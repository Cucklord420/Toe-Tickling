// SPDX-License-Identifier: AGPL-3.0-only
//! Protected, handle-anchored filesystem access for adoption mutations.

use super::{JournalFileState, JournalPermissions};
#[cfg(not(windows))]
use cap_fs_ext::DirExt;
#[cfg(any(unix, windows))]
use cap_fs_ext::OsMetadataExt;
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
#[cfg(any(target_os = "linux", windows))]
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Shared directory handles pinned for the lifetime of an apply or recovery.
pub(super) struct MutationContext {
    target: AnchoredDir,
    git: AnchoredDir,
    #[cfg(windows)]
    dacl: windows_acl::DaclPolicy,
}

struct AnchoredDir {
    path: PathBuf,
    _ancestors: Vec<Dir>,
    root: Arc<Dir>,
    pinned: Mutex<HashMap<PathBuf, Arc<Dir>>>,
}

struct PinnedParent {
    directory: Arc<Dir>,
    leaf: OsString,
}

impl MutationContext {
    pub(super) fn ensure_supported() -> super::Result<()> {
        #[cfg(target_os = "macos")]
        return Err("Guidebook adoption mutations are paused on macOS because ACL safety cannot yet be proven".into());

        #[cfg(windows)]
        return Ok(());

        #[cfg(target_os = "linux")]
        {
            if rustix::process::geteuid().as_raw() == 0 {
                return Err("Guidebook adoption mutations are paused when run as root".into());
            }
            Ok(())
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        Err("Guidebook adoption mutations are paused on this platform because path safety cannot be proven".into())
    }

    pub(super) fn open(target: &Path, git_dir: &Path) -> super::Result<Self> {
        Self::ensure_supported()?;
        #[cfg(windows)]
        let dacl = windows_acl::DaclPolicy::current()?;

        let target = AnchoredDir::open(
            target,
            #[cfg(windows)]
            &[],
            #[cfg(windows)]
            &dacl,
        )?;
        #[cfg(windows)]
        let git_protected_roots = vec![target.path.clone()];
        let git = AnchoredDir::open(
            git_dir,
            #[cfg(windows)]
            &git_protected_roots,
            #[cfg(windows)]
            &dacl,
        )?;
        Ok(Self {
            target,
            git,
            #[cfg(windows)]
            dacl,
        })
    }

    pub(super) fn target_path(&self) -> &Path {
        &self.target.path
    }

    pub(super) fn git_path(&self) -> &Path {
        &self.git.path
    }

    fn anchor(&self, path: &Path) -> super::Result<(&AnchoredDir, PathBuf)> {
        // Git metadata can be outside the working tree for linked worktrees;
        // prefer that anchor for paths which are inside both trees.
        if let Ok(relative) = path.strip_prefix(&self.git.path) {
            return Ok((&self.git, relative.to_path_buf()));
        }
        if let Ok(relative) = path.strip_prefix(&self.target.path) {
            return Ok((&self.target, relative.to_path_buf()));
        }
        Err(format!(
            "Mutation path is outside the verified target and Git metadata roots: {}",
            path.display()
        ))
    }

    pub(super) fn file_state(&self, path: &Path) -> super::Result<Option<JournalFileState>> {
        let (anchor, relative) = self.anchor(path)?;
        let Some(parent) = anchor.try_parent(
            &relative,
            #[cfg(windows)]
            &self.dacl,
        )?
        else {
            return Ok(None);
        };
        let before = match parent.directory.symlink_metadata(&parent.leaf) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Cannot inspect {}: {error}", path.display())),
        };
        if before.file_type().is_symlink() || !before.is_file() {
            return Err(format!("Expected a regular file at {}", path.display()));
        }
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let mut file = parent
            .directory
            .open_with(&parent.leaf, &options)
            .map_err(|error| format!("Cannot safely open {}: {error}", path.display()))?;
        let opened = file
            .metadata()
            .map_err(|error| format!("Cannot inspect opened file {}: {error}", path.display()))?;
        verify_file(
            &file,
            &opened,
            #[cfg(windows)]
            &self.dacl,
        )
        .map_err(|error| {
            format!(
                "Mutation file {} failed its security check: {error}",
                path.display()
            )
        })?;
        same_file(&before, &opened)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
        Ok(Some(JournalFileState {
            bytes: Some(bytes),
            permissions: Some(journal_permissions(&opened)),
        }))
    }

    pub(super) fn read(&self, path: &Path) -> super::Result<Vec<u8>> {
        self.file_state(path)?
            .and_then(|state| state.bytes)
            .ok_or_else(|| format!("File does not exist: {}", path.display()))
    }

    pub(super) fn sync_file(&self, path: &Path) -> super::Result<()> {
        let (anchor, relative) = self.anchor(path)?;
        let parent = anchor.parent(
            &relative,
            false,
            #[cfg(windows)]
            &self.dacl,
        )?;
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true).write(true).follow(FollowSymlinks::No);
        let file = parent
            .directory
            .open_with(&parent.leaf, &options)
            .map_err(|error| format!("Cannot open {} for syncing: {error}", path.display()))?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        verify_file(
            &file,
            &metadata,
            #[cfg(windows)]
            &self.dacl,
        )?;
        file.sync_all()
            .map_err(|error| format!("Cannot sync {}: {error}", path.display()))
    }

    pub(super) fn validate_git_identity(&self, expected: &super::GitIdentity) -> super::Result<()> {
        #[cfg(windows)]
        if !expected.target.has_stable_id() || !expected.marker.has_stable_id() {
            return Err(
                "Windows filesystem identity cannot be proven by stable volume and file IDs".into(),
            );
        }
        self.pin_git_marker()?;
        let current = super::GitIdentity::read(&self.target.path)?;
        if current != *expected {
            return Err(
                "Git working-tree identity changed after the mutation handles were opened".into(),
            );
        }
        Ok(())
    }

    pub(super) fn pin_git_marker(&self) -> super::Result<()> {
        let marker = self.target.path.join(".git");
        let metadata = self
            .target
            .root
            .symlink_metadata(".git")
            .map_err(|error| format!("Cannot inspect Git working-tree marker: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("Git working-tree marker cannot be a symlink".into());
        }
        if metadata.is_dir() {
            self.pin_directory(&marker)
        } else if metadata.is_file() {
            let _ = self
                .file_state(&marker)?
                .ok_or("Git working-tree marker disappeared")?;
            Ok(())
        } else {
            Err("Git working-tree marker must be a regular file or directory".into())
        }
    }

    fn pin_directory(&self, path: &Path) -> super::Result<()> {
        let (anchor, relative) = self.anchor(path)?;
        let components = validated_components(&relative)?;
        if components.is_empty() {
            return Ok(());
        }
        let name = components
            .last()
            .cloned()
            .ok_or("Missing directory component")?;
        let parent = anchor
            .parent_components_existing(
                &components[..components.len() - 1],
                #[cfg(windows)]
                &self.dacl,
            )?
            .ok_or("Directory parent disappeared")?;
        let metadata = parent
            .symlink_metadata(&name)
            .map_err(|error| format!("Cannot inspect directory {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!("Refusing unsafe directory: {}", path.display()));
        }
        check_transition(
            &parent,
            &metadata,
            #[cfg(windows)]
            &self.dacl,
            #[cfg(windows)]
            windows_acl::DaclScope::ProtectedObject,
        )?;
        let child = open_directory_nofollow(&parent, &name).map_err(|error| {
            format!(
                "Cannot open directory {} without following links: {error}",
                path.display()
            )
        })?;
        verify_directory(
            &child,
            &metadata,
            #[cfg(windows)]
            &self.dacl,
            #[cfg(windows)]
            windows_acl::DaclScope::ProtectedObject,
        )?;
        let relative = PathBuf::from_iter(components);
        anchor
            .pinned
            .lock()
            .map_err(|_| "Mutation directory cache was poisoned")?
            .insert(relative, Arc::new(child));
        Ok(())
    }

    pub(super) fn write_new(&self, path: &Path, contents: &[u8]) -> super::Result<()> {
        let (anchor, relative) = self.anchor(path)?;
        let parent = anchor.parent(
            &relative,
            true,
            #[cfg(windows)]
            &self.dacl,
        )?;
        let mut file = create_file_entry(
            &parent.directory,
            &parent.leaf,
            #[cfg(windows)]
            &self.dacl,
        )
        .map_err(|error| format!("Cannot create {}: {error}", path.display()))?;
        let verification = file
            .metadata()
            .map_err(|error| format!("Cannot inspect new file {}: {error}", path.display()))
            .and_then(|metadata| {
                verify_created_file(
                    &file,
                    &metadata,
                    #[cfg(windows)]
                    &self.dacl,
                )
            });
        if let Err(error) = verification {
            let cleanup = cleanup_created_file(
                &parent.directory,
                &parent.leaf,
                &file,
                #[cfg(windows)]
                &self.dacl,
            );
            drop(file);
            return Err(match cleanup {
                Ok(()) => format!("Cannot safely use new file {}: {error}", path.display()),
                Err(cleanup_error) => format!(
                    "Cannot safely use new file {}: {error}; cleanup also failed: {cleanup_error}",
                    path.display()
                ),
            });
        }
        if let Err(error) = file.write_all(contents).and_then(|()| file.sync_all()) {
            let cleanup = cleanup_created_file(
                &parent.directory,
                &parent.leaf,
                &file,
                #[cfg(windows)]
                &self.dacl,
            );
            drop(file);
            return Err(match cleanup {
                Ok(()) => format!("Cannot persist {}: {error}", path.display()),
                Err(cleanup_error) => format!(
                    "Cannot persist {}: {error}; cleanup also failed: {cleanup_error}",
                    path.display()
                ),
            });
        }
        Ok(())
    }

    pub(super) fn write_atomic(
        &self,
        path: &Path,
        contents: &[u8],
        create_only: bool,
    ) -> super::Result<()> {
        self.write_atomic_with_hook(path, contents, create_only, || Ok(()))
    }

    #[cfg(all(test, windows))]
    fn write_atomic_with_pre_rename_hook<F>(
        &self,
        path: &Path,
        contents: &[u8],
        create_only: bool,
        before_rename: F,
    ) -> super::Result<()>
    where
        F: FnOnce() -> super::Result<()>,
    {
        self.write_atomic_with_hook(path, contents, create_only, before_rename)
    }

    fn write_atomic_with_hook<F>(
        &self,
        path: &Path,
        contents: &[u8],
        create_only: bool,
        before_rename: F,
    ) -> super::Result<()>
    where
        F: FnOnce() -> super::Result<()>,
    {
        let (anchor, relative) = self.anchor(path)?;
        let parent = anchor.parent(
            &relative,
            true,
            #[cfg(windows)]
            &self.dacl,
        )?;
        let temp = create_temp(
            &parent,
            contents,
            #[cfg(windows)]
            &self.dacl,
        )?;
        let temp_path = path.with_file_name(&temp);
        #[cfg(windows)]
        let source_state = self
            .file_state(&temp_path)?
            .ok_or("Temporary file disappeared before publication")?;
        #[cfg(windows)]
        let destination_state = self.file_state(path)?;
        let result = {
            #[cfg(windows)]
            {
                if create_only && destination_state.is_some() {
                    Err(format!(
                        "Cannot publish {} because it already exists",
                        path.display()
                    ))
                } else if !create_only && destination_state.is_none() {
                    Err(format!(
                        "Cannot replace {} because it disappeared",
                        path.display()
                    ))
                } else {
                    rename_file_relative_with_hook(
                        &parent.directory,
                        &temp,
                        &parent.directory,
                        &parent.leaf,
                        &source_state,
                        destination_state.as_ref(),
                        &self.dacl,
                        before_rename,
                    )
                    .map_err(|error| {
                        format!("Cannot atomically publish {}: {error}", path.display())
                    })
                }
            }
            #[cfg(not(windows))]
            {
                before_rename()?;
                if create_only {
                    // A hard-link publishes the fully synced inode atomically without
                    // replacing an existing recovery journal.
                    parent
                        .directory
                        .hard_link(&temp, &parent.directory, &parent.leaf)
                        .map_err(|error| {
                            format!(
                                "Cannot publish {} without replacement: {error}",
                                path.display()
                            )
                        })
                } else {
                    parent
                        .directory
                        .rename(&temp, &parent.directory, &parent.leaf)
                        .map_err(|error| {
                            format!("Cannot atomically replace {}: {error}", path.display())
                        })
                }
            }
        };
        let cleanup = {
            #[cfg(windows)]
            if result.is_err() {
                self.remove_file(&temp_path, &source_state)
            } else {
                Ok(())
            }
            #[cfg(not(windows))]
            if create_only || result.is_err() {
                let state = self
                    .file_state(&temp_path)?
                    .ok_or("Temporary file disappeared before cleanup")?;
                self.remove_file(&temp_path, &state)
            } else {
                Ok(())
            }
        };
        match (result, cleanup) {
            (Ok(()), Ok(())) => {}
            (Ok(()), Err(error)) => {
                return Err(format!(
                    "Published {} but cannot remove its temporary link: {error}",
                    path.display()
                ));
            }
            (Err(error), Ok(())) => return Err(error),
            (Err(error), Err(cleanup_error)) => {
                return Err(format!(
                    "{error}; cannot remove temporary file beside {}: {cleanup_error}",
                    path.display()
                ));
            }
        }
        let published = self.file_state(path)?.ok_or("Published file disappeared")?;
        if published.bytes.as_deref() != Some(contents) {
            return Err(format!(
                "Published file contents did not match the staged bytes: {}",
                path.display()
            ));
        }
        Ok(())
    }

    pub(super) fn remove_file(
        &self,
        path: &Path,
        expected: &JournalFileState,
    ) -> super::Result<()> {
        self.remove_file_with_hook(path, expected, || Ok(()))
    }

    #[cfg(all(test, windows))]
    fn remove_file_with_pre_delete_hook<F>(
        &self,
        path: &Path,
        expected: &JournalFileState,
        before_delete: F,
    ) -> super::Result<()>
    where
        F: FnOnce() -> super::Result<()>,
    {
        self.remove_file_with_hook(path, expected, before_delete)
    }

    fn remove_file_with_hook<F>(
        &self,
        path: &Path,
        expected: &JournalFileState,
        before_delete: F,
    ) -> super::Result<()>
    where
        F: FnOnce() -> super::Result<()>,
    {
        let (anchor, relative) = self.anchor(path)?;
        let Some(parent) = anchor.try_parent(
            &relative,
            #[cfg(windows)]
            &self.dacl,
        )?
        else {
            return Ok(());
        };
        let Some(current) = self.file_state(path)? else {
            return Ok(());
        };
        if current != *expected {
            return Err(format!(
                "Refusing to remove a changed mutation file: {}",
                path.display()
            ));
        }
        #[cfg(windows)]
        {
            let mut file = open_mutation_file(&parent.directory, &parent.leaf, &self.dacl)?;
            let opened = opened_file_state(&mut file, &self.dacl)?;
            if opened != *expected {
                return Err(format!(
                    "Mutation file changed before handle-relative removal: {}",
                    path.display()
                )
                .into());
            }
            before_delete()?;
            delete_open_handle(file.as_raw_handle())
        }
        #[cfg(not(windows))]
        {
            before_delete()?;
            parent
                .directory
                .remove_file(&parent.leaf)
                .map_err(|error| format!("Cannot remove {}: {error}", path.display()))
        }
    }

    pub(super) fn rename(
        &self,
        from: &Path,
        to: &Path,
        expected_source: &JournalFileState,
        expected_destination: Option<&JournalFileState>,
    ) -> super::Result<()> {
        self.rename_with_hook(from, to, expected_source, expected_destination, || Ok(()))
    }

    #[cfg(all(test, windows))]
    fn rename_with_pre_rename_hook<F>(
        &self,
        from: &Path,
        to: &Path,
        expected_source: &JournalFileState,
        expected_destination: Option<&JournalFileState>,
        before_rename: F,
    ) -> super::Result<()>
    where
        F: FnOnce() -> super::Result<()>,
    {
        self.rename_with_hook(
            from,
            to,
            expected_source,
            expected_destination,
            before_rename,
        )
    }

    fn rename_with_hook<F>(
        &self,
        from: &Path,
        to: &Path,
        expected_source: &JournalFileState,
        expected_destination: Option<&JournalFileState>,
        before_rename: F,
    ) -> super::Result<()>
    where
        F: FnOnce() -> super::Result<()>,
    {
        let (from_anchor, from_relative) = self.anchor(from)?;
        let (to_anchor, to_relative) = self.anchor(to)?;
        if !std::ptr::eq(from_anchor, to_anchor) {
            return Err("Atomic mutation rename cannot cross verified directory roots".into());
        }
        let from_parent = from_anchor.parent(
            &from_relative,
            false,
            #[cfg(windows)]
            &self.dacl,
        )?;
        let to_parent = to_anchor.parent(
            &to_relative,
            true,
            #[cfg(windows)]
            &self.dacl,
        )?;
        if self.file_state(from)?.as_ref() != Some(expected_source)
            || self.file_state(to)?.as_ref() != expected_destination
        {
            return Err("Mutation source or destination changed before rename".into());
        }
        #[cfg(windows)]
        {
            return rename_file_relative_with_hook(
                &from_parent.directory,
                &from_parent.leaf,
                &to_parent.directory,
                &to_parent.leaf,
                expected_source,
                expected_destination,
                &self.dacl,
                before_rename,
            );
        }
        #[cfg(not(windows))]
        {
            before_rename()?;
            from_parent
                .directory
                .rename(&from_parent.leaf, &to_parent.directory, &to_parent.leaf)
                .map_err(|error| {
                    format!(
                        "Cannot atomically move {} to {}: {error}",
                        from.display(),
                        to.display()
                    )
                })
        }
    }

    pub(super) fn set_permissions(
        &self,
        path: &Path,
        expected: &JournalPermissions,
    ) -> super::Result<()> {
        expected.validate()?;
        let (anchor, relative) = self.anchor(path)?;
        let parent = anchor.parent(
            &relative,
            false,
            #[cfg(windows)]
            &self.dacl,
        )?;
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        #[cfg(windows)]
        {
            use cap_std::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_READ_ATTRIBUTES, FILE_WRITE_ATTRIBUTES, READ_CONTROL, SYNCHRONIZE,
            };
            options.access_mode(
                READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES | SYNCHRONIZE,
            );
        }
        let file = parent
            .directory
            .open_with(&parent.leaf, &options)
            .map_err(|error| {
                format!("Cannot open {} to set permissions: {error}", path.display())
            })?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        verify_file(
            &file,
            &metadata,
            #[cfg(windows)]
            &self.dacl,
        )?;
        let mut permissions = metadata.permissions();
        #[cfg(unix)]
        {
            use cap_std::fs::PermissionsExt;
            permissions.set_mode(expected.unix_mode.ok_or("Missing Unix mode")?);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(expected.readonly);
        file.set_permissions(permissions)
            .map_err(|error| format!("Cannot set permissions on {}: {error}", path.display()))
    }

    pub(super) fn create_parent_dirs(&self, path: &Path) -> super::Result<()> {
        let (anchor, relative) = self.anchor(path)?;
        let _ = anchor.parent(
            &relative,
            true,
            #[cfg(windows)]
            &self.dacl,
        )?;
        Ok(())
    }
}

#[cfg(windows)]
pub(super) fn windows_file_identity(
    path: &Path,
) -> super::Result<(Option<std::time::SystemTime>, u64, [u8; 16])> {
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdInfo,
            GetFileInformationByHandleEx, READ_CONTROL, SYNCHRONIZE,
        },
    };

    let mut options = std::fs::OpenOptions::new();
    options
        .read(true)
        .access_mode(FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options.open(path).map_err(|error| {
        format!(
            "Cannot safely open Git identity {}: {error}",
            path.display()
        )
    })?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Cannot inspect Git identity {}: {error}", path.display()))?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || !(metadata.is_file() || metadata.is_dir())
    {
        return Err(format!(
            "Git identity is a reparse point or special file: {}",
            path.display()
        ));
    }
    let mut information = FILE_ID_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as HANDLE,
            FileIdInfo,
            (&mut information as *mut FILE_ID_INFO).cast::<std::ffi::c_void>(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
    {
        return Err(format!(
            "Cannot verify stable Windows file identity for {}",
            path.display()
        ));
    }
    let file_id = information.FileId.Identifier;
    if information.VolumeSerialNumber == 0 || file_id == [0; 16] {
        return Err(format!(
            "Windows did not provide a stable file identity for {}",
            path.display()
        ));
    }
    Ok((
        metadata.created().ok(),
        information.VolumeSerialNumber,
        file_id,
    ))
}

#[cfg(windows)]
fn create_directory_relative(
    parent: &Dir,
    name: &OsStr,
    policy: &windows_acl::DaclPolicy,
) -> std::io::Result<Dir> {
    use std::os::{
        windows::ffi::OsStrExt,
        windows::io::{FromRawHandle, OwnedHandle},
    };
    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_CREATE, FILE_DIRECTORY_FILE, FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
            },
        },
        Win32::{
            Foundation::{
                OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, STATUS_OBJECT_NAME_COLLISION,
                STATUS_SUCCESS, UNICODE_STRING,
            },
            Storage::FileSystem::{
                DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, READ_CONTROL, SYNCHRONIZE,
            },
        },
    };

    let mut wide: Vec<u16> = name.encode_wide().collect();
    if wide.is_empty() || wide.iter().any(|unit| *unit == 0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Directory component is empty or contains a NUL",
        ));
    }
    let Some(name_length) = wide.len().checked_mul(std::mem::size_of::<u16>()) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Directory component length overflowed",
        ));
    };
    let Ok(name_length) = u16::try_from(name_length) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Directory component exceeds the Windows name limit",
        ));
    };
    let security = policy
        .creation_security_descriptor()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut object_name = UNICODE_STRING {
        Length: name_length,
        MaximumLength: name_length,
        Buffer: wide.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle() as _,
        ObjectName: &mut object_name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: security.as_ptr(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle = std::ptr::null_mut();
    let mut status_block = windows_sys::Win32::System::IO::IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            FILE_LIST_DIRECTORY
                | FILE_TRAVERSE
                | FILE_READ_ATTRIBUTES
                | READ_CONTROL
                | SYNCHRONIZE
                | DELETE,
            &attributes,
            &mut status_block,
            std::ptr::null(),
            FILE_ATTRIBUTE_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if status != STATUS_SUCCESS {
        if status == STATUS_OBJECT_NAME_COLLISION {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Directory component already exists",
            ));
        }
        let code = unsafe { RtlNtStatusToDosError(status) };
        return Err(std::io::Error::from_raw_os_error(code as i32));
    }
    if handle.is_null() {
        return Err(std::io::Error::other(
            "NtCreateFile returned success without a directory handle",
        ));
    }
    let owned = unsafe { OwnedHandle::from_raw_handle(handle as _) };
    Ok(Dir::from(owned))
}

#[cfg(windows)]
fn create_file_relative(
    parent: &Dir,
    name: &OsStr,
    policy: &windows_acl::DaclPolicy,
) -> std::io::Result<cap_std::fs::File> {
    use std::os::{
        windows::ffi::OsStrExt,
        windows::io::{FromRawHandle, OwnedHandle},
    };
    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_CREATE, FILE_NON_DIRECTORY_FILE, FILE_OPEN_REPARSE_POINT,
                FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
            },
        },
        Win32::{
            Foundation::{
                OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, STATUS_OBJECT_NAME_COLLISION,
                STATUS_SUCCESS, UNICODE_STRING,
            },
            Storage::FileSystem::{
                DELETE, FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
                FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_DATA, READ_CONTROL, SYNCHRONIZE,
            },
        },
    };

    let mut wide: Vec<u16> = name.encode_wide().collect();
    if wide.is_empty() || wide.iter().any(|unit| *unit == 0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "File component is empty or contains a NUL",
        ));
    }
    let Some(name_length) = wide.len().checked_mul(std::mem::size_of::<u16>()) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "File component length overflowed",
        ));
    };
    let Ok(name_length) = u16::try_from(name_length) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "File component exceeds the Windows name limit",
        ));
    };
    let security = policy
        .creation_security_descriptor()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut object_name = UNICODE_STRING {
        Length: name_length,
        MaximumLength: name_length,
        Buffer: wide.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle() as _,
        ObjectName: &mut object_name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: security.as_ptr(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle = std::ptr::null_mut();
    let mut status_block = windows_sys::Win32::System::IO::IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            DELETE
                | FILE_READ_DATA
                | FILE_WRITE_DATA
                | FILE_READ_ATTRIBUTES
                | READ_CONTROL
                | SYNCHRONIZE,
            &attributes,
            &mut status_block,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if status != STATUS_SUCCESS {
        if status == STATUS_OBJECT_NAME_COLLISION {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "File component already exists",
            ));
        }
        let code = unsafe { RtlNtStatusToDosError(status) };
        return Err(std::io::Error::from_raw_os_error(code as i32));
    }
    if handle.is_null() {
        return Err(std::io::Error::other(
            "NtCreateFile returned success without a file handle",
        ));
    }
    let owned = unsafe { OwnedHandle::from_raw_handle(handle as _) };
    Ok(cap_std::fs::File::from(owned))
}

#[cfg(windows)]
fn open_directory_relative(parent: &Dir, name: &OsStr) -> std::io::Result<Dir> {
    use std::os::{
        windows::ffi::OsStrExt,
        windows::io::{FromRawHandle, OwnedHandle},
    };
    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
            },
        },
        Win32::{
            Foundation::{
                OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, STATUS_SUCCESS, UNICODE_STRING,
            },
            Storage::FileSystem::{
                FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
                READ_CONTROL, SYNCHRONIZE,
            },
        },
    };

    let mut wide: Vec<u16> = name.encode_wide().collect();
    if wide.is_empty() || wide.iter().any(|unit| *unit == 0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Directory component is empty or contains a NUL",
        ));
    }
    let Some(name_length) = wide.len().checked_mul(std::mem::size_of::<u16>()) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Directory component length overflowed",
        ));
    };
    let Ok(name_length) = u16::try_from(name_length) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Directory component exceeds the Windows name limit",
        ));
    };
    let mut object_name = UNICODE_STRING {
        Length: name_length,
        MaximumLength: name_length,
        Buffer: wide.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle() as _,
        ObjectName: &mut object_name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle = std::ptr::null_mut();
    let mut status_block = windows_sys::Win32::System::IO::IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE,
            &attributes,
            &mut status_block,
            std::ptr::null(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_OPEN,
            FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if status != STATUS_SUCCESS {
        let code = unsafe { RtlNtStatusToDosError(status) };
        return Err(std::io::Error::from_raw_os_error(code as i32));
    }
    if handle.is_null() {
        return Err(std::io::Error::other(
            "NtCreateFile returned success without a directory handle",
        ));
    }
    let owned = unsafe { OwnedHandle::from_raw_handle(handle as _) };
    Ok(Dir::from(owned))
}

fn open_directory_nofollow(parent: &Dir, name: &OsStr) -> std::io::Result<Dir> {
    #[cfg(windows)]
    {
        open_directory_relative(parent, name)
    }
    #[cfg(not(windows))]
    {
        parent.open_dir_nofollow(name)
    }
}

#[cfg(windows)]
fn delete_open_handle(raw: std::os::windows::io::RawHandle) -> super::Result<()> {
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
        },
    };

    let mut disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    if unsafe {
        SetFileInformationByHandle(
            raw as HANDLE,
            FileDispositionInfo,
            (&mut disposition as *mut FILE_DISPOSITION_INFO).cast::<std::ffi::c_void>(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } == 0
    {
        return Err(format!(
            "Cannot mark the opened mutation object for deletion: {}",
            std::io::Error::last_os_error()
        )
        .into());
    }
    Ok(())
}

fn cleanup_created_file(
    parent: &Dir,
    name: &OsStr,
    file: &cap_std::fs::File,
    #[cfg(windows)] _policy: &windows_acl::DaclPolicy,
) -> super::Result<()> {
    #[cfg(windows)]
    {
        let _ = (parent, name);
        delete_open_handle(file.as_raw_handle())
    }
    #[cfg(not(windows))]
    {
        let _ = file;
        parent
            .remove_file(name)
            .map_err(|error| format!("Cannot remove newly created file: {error}"))
    }
}

#[cfg(windows)]
fn open_mutation_file(
    parent: &Dir,
    name: &OsStr,
    policy: &windows_acl::DaclPolicy,
) -> super::Result<cap_std::fs::File> {
    use cap_std::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
        FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, SYNCHRONIZE,
    };

    let mut options = cap_std::fs::OpenOptions::new();
    options
        .read(true)
        .access_mode(DELETE | FILE_READ_DATA | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    let file = parent
        .open_with(name, &options)
        .map_err(|error| format!("Cannot open mutation file without following links: {error}"))?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    verify_file(&file, &metadata, policy)?;
    Ok(file)
}

#[cfg(windows)]
fn opened_file_state(
    file: &mut cap_std::fs::File,
    policy: &windows_acl::DaclPolicy,
) -> super::Result<JournalFileState> {
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    verify_file(file, &metadata, policy)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read opened mutation file: {error}"))?;
    Ok(JournalFileState {
        bytes: Some(bytes),
        permissions: Some(journal_permissions(&metadata)),
    })
}

#[cfg(windows)]
fn rename_file_relative_with_hook<F>(
    source_parent: &Dir,
    source_name: &OsStr,
    destination_parent: &Dir,
    destination_name: &OsStr,
    expected_source: &JournalFileState,
    expected_destination: Option<&JournalFileState>,
    policy: &windows_acl::DaclPolicy,
    before_rename: F,
) -> super::Result<()>
where
    F: FnOnce() -> super::Result<()>,
{
    let mut source = open_mutation_file(source_parent, source_name, policy)?;
    if opened_file_state(&mut source, policy)? != *expected_source {
        return Err("Mutation source changed before handle-relative rename".into());
    }

    match expected_destination {
        Some(expected) => {
            let mut destination = open_mutation_file(destination_parent, destination_name, policy)?;
            if opened_file_state(&mut destination, policy)? != *expected {
                return Err("Mutation destination changed before handle-relative rename".into());
            }
        }
        None => match destination_parent.symlink_metadata(destination_name) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err("Unexpected destination appeared before handle-relative rename".into());
            }
            Err(error) => {
                return Err(format!("Cannot verify absent mutation destination: {error}").into());
            }
        },
    }

    before_rename()?;
    rename_open_file_relative(
        &source,
        destination_parent,
        destination_name,
        expected_destination.is_some(),
    )
}

#[cfg(windows)]
fn rename_open_file_relative(
    file: &cap_std::fs::File,
    destination_parent: &Dir,
    destination_name: &OsStr,
    replace_if_exists: bool,
) -> super::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::{
        Wdk::Storage::FileSystem::{
            FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0, FileRenameInformation,
            NtSetInformationFile,
        },
        Win32::{
            Foundation::{HANDLE, RtlNtStatusToDosError, STATUS_SUCCESS},
            System::IO::IO_STATUS_BLOCK,
        },
    };

    let wide: Vec<u16> = destination_name.encode_wide().collect();
    if wide.is_empty() || wide.iter().any(|unit| *unit == 0) {
        return Err("Destination name is empty or contains a NUL".into());
    }
    let name_length = wide
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|length| u32::try_from(length).ok())
        .ok_or("Destination name length overflowed")?;
    let filename_bytes = wide
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or("Rename filename length overflowed")?;
    // Windows requires the complete structure size plus the variable filename
    // bytes, including any trailing padding in the structure.
    let buffer_length = std::mem::size_of::<FILE_RENAME_INFORMATION>()
        .checked_add(filename_bytes)
        .ok_or("Rename information length overflowed")?;
    let buffer_length_u32 = u32::try_from(buffer_length)
        .map_err(|_| "Rename information exceeds the Windows buffer limit")?;
    let words = buffer_length.div_ceil(std::mem::size_of::<usize>());
    let mut buffer = vec![0usize; words];
    let information = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    unsafe {
        (*information).Anonymous = FILE_RENAME_INFORMATION_0 {
            ReplaceIfExists: replace_if_exists,
        };
        (*information).RootDirectory = destination_parent.as_raw_handle() as HANDLE;
        (*information).FileNameLength = name_length;
        std::ptr::copy_nonoverlapping(
            wide.as_ptr(),
            (*information).FileName.as_mut_ptr(),
            wide.len(),
        );
    }
    let mut status_block = IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtSetInformationFile(
            file.as_raw_handle() as HANDLE,
            &mut status_block,
            information.cast::<std::ffi::c_void>(),
            buffer_length_u32,
            FileRenameInformation,
        )
    };
    if status != STATUS_SUCCESS {
        let code = unsafe { RtlNtStatusToDosError(status) };
        return Err(format!(
            "Cannot rename through the verified destination handle: {}",
            std::io::Error::from_raw_os_error(code as i32)
        )
        .into());
    }
    Ok(())
}

impl AnchoredDir {
    fn open(
        path: &Path,
        #[cfg(windows)] protected_roots: &[PathBuf],
        #[cfg(windows)] policy: &windows_acl::DaclPolicy,
    ) -> super::Result<Self> {
        if !path.is_absolute() {
            return Err(format!(
                "Mutation root must be absolute: {}",
                path.display()
            ));
        }
        #[cfg(target_os = "linux")]
        let volume_root = PathBuf::from("/");
        #[cfg(windows)]
        let volume_root = windows_acl::local_volume_root(path)?;
        #[cfg(not(any(target_os = "linux", windows)))]
        return Err("Protected mutation paths are unavailable on this platform".into());

        #[cfg(any(target_os = "linux", windows))]
        {
            let current =
                Dir::open_ambient_dir(&volume_root, ambient_authority()).map_err(|error| {
                    format!(
                        "Cannot open filesystem root {}: {error}",
                        volume_root.display()
                    )
                })?;
            #[cfg(windows)]
            let root_scope = dacl_scope_for_path(&volume_root, path, protected_roots);
            verify_directory(
                &current,
                &current.dir_metadata().map_err(|error| error.to_string())?,
                #[cfg(windows)]
                policy,
                #[cfg(windows)]
                root_scope,
            )
            .map_err(|error| {
                format!(
                    "Mutation volume root {} is not protected: {error}",
                    volume_root.display()
                )
            })?;
            let mut ancestors = vec![current];
            let mut current_path = volume_root.clone();
            let relative = path.strip_prefix(&volume_root).map_err(|error| {
                format!("Mutation path has an unsupported volume prefix: {error}")
            })?;
            for component in relative.components() {
                let Component::Normal(name) = component else {
                    return Err(format!(
                        "Mutation root has an unsafe component: {}",
                        path.display()
                    ));
                };
                let parent = ancestors.last().ok_or("Missing filesystem root handle")?;
                let before = parent.symlink_metadata(name).map_err(|error| {
                    format!(
                        "Cannot inspect ancestry component {}: {error}",
                        name.to_string_lossy()
                    )
                })?;
                if before.file_type().is_symlink() || !before.is_dir() {
                    return Err(format!(
                        "Refusing symlink or non-directory ancestry component: {}",
                        name.to_string_lossy()
                    ));
                }
                check_transition(
                    parent,
                    &before,
                    #[cfg(windows)]
                    policy,
                    #[cfg(windows)]
                    dacl_scope_for_path(&current_path, path, protected_roots),
                )
                .map_err(|error| {
                    format!(
                        "Mutation ancestry parent {} is not protected: {error}",
                        current_path.display()
                    )
                })?;
                let child = open_directory_nofollow(parent, name).map_err(|error| {
                    format!(
                        "Cannot open ancestry component {} without following links: {error}",
                        name.to_string_lossy()
                    )
                })?;
                current_path.push(name);
                verify_directory(
                    &child,
                    &before,
                    #[cfg(windows)]
                    policy,
                    #[cfg(windows)]
                    dacl_scope_for_path(&current_path, path, protected_roots),
                )
                .map_err(|error| {
                    format!(
                        "Mutation ancestry {} is not protected: {error}",
                        current_path.display()
                    )
                })?;
                ancestors.push(child);
            }
            let root = Arc::new(ancestors.pop().ok_or("Missing mutation root handle")?);
            Ok(Self {
                path: path.to_path_buf(),
                _ancestors: ancestors,
                root,
                pinned: Mutex::new(HashMap::new()),
            })
        }
    }

    fn parent(
        &self,
        relative: &Path,
        create: bool,
        #[cfg(windows)] policy: &windows_acl::DaclPolicy,
    ) -> super::Result<PinnedParent> {
        let components = validated_components(relative)?;
        if components.is_empty() {
            return Err("Mutation path has no file name".into());
        }
        let mut parents = components;
        let leaf = parents.pop().ok_or("Mutation path has no file name")?;
        let directory = self.parent_components(
            &parents,
            create,
            #[cfg(windows)]
            policy,
        )?;
        Ok(PinnedParent { directory, leaf })
    }

    fn try_parent(
        &self,
        relative: &Path,
        #[cfg(windows)] policy: &windows_acl::DaclPolicy,
    ) -> super::Result<Option<PinnedParent>> {
        let components = validated_components(relative)?;
        if components.is_empty() {
            return Err("Mutation path has no file name".into());
        }
        let leaf = components
            .last()
            .cloned()
            .ok_or("Mutation path has no file name")?;
        let Some(directory) = self.parent_components_existing(
            &components[..components.len() - 1],
            #[cfg(windows)]
            policy,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(PinnedParent { directory, leaf }))
    }

    fn parent_components_existing(
        &self,
        components: &[OsString],
        #[cfg(windows)] policy: &windows_acl::DaclPolicy,
    ) -> super::Result<Option<Arc<Dir>>> {
        let mut cache = self
            .pinned
            .lock()
            .map_err(|_| "Mutation directory cache was poisoned")?;
        let mut current = Arc::clone(&self.root);
        let mut relative = PathBuf::new();
        for name in components {
            relative.push(name);
            if let Some(cached) = cache.get(&relative) {
                current = Arc::clone(cached);
                continue;
            }
            let metadata = match current.symlink_metadata(name) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => {
                    return Err(format!(
                        "Cannot inspect mutation parent {}: {error}",
                        relative.display()
                    ));
                }
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(format!(
                    "Refusing symlink or non-directory mutation parent: {}",
                    relative.display()
                ));
            }
            check_transition(
                &current,
                &metadata,
                #[cfg(windows)]
                policy,
                #[cfg(windows)]
                windows_acl::DaclScope::ProtectedObject,
            )?;
            let child = open_directory_nofollow(&current, name).map_err(|error| {
                format!(
                    "Cannot open mutation parent {} without following links: {error}",
                    relative.display()
                )
            })?;
            verify_directory(
                &child,
                &metadata,
                #[cfg(windows)]
                policy,
                #[cfg(windows)]
                windows_acl::DaclScope::ProtectedObject,
            )?;
            let child = Arc::new(child);
            cache.insert(relative.clone(), Arc::clone(&child));
            current = child;
        }
        Ok(Some(current))
    }

    fn parent_components(
        &self,
        components: &[OsString],
        create: bool,
        #[cfg(windows)] policy: &windows_acl::DaclPolicy,
    ) -> super::Result<Arc<Dir>> {
        let mut cache = self
            .pinned
            .lock()
            .map_err(|_| "Mutation directory cache was poisoned")?;
        let mut current = Arc::clone(&self.root);
        let mut relative = PathBuf::new();
        for name in components {
            relative.push(name);
            if let Some(cached) = cache.get(&relative) {
                current = Arc::clone(cached);
                continue;
            }
            let before = match current.symlink_metadata(name) {
                Ok(metadata) => Some(metadata),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => None,
                Err(error) => {
                    return Err(format!(
                        "Cannot inspect mutation parent {}: {error}",
                        relative.display()
                    ));
                }
            };
            let (child, metadata, created_here) = match before {
                Some(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_dir() {
                        return Err(format!(
                            "Refusing symlink or non-directory mutation parent: {}",
                            relative.display()
                        ));
                    }
                    let child = open_directory_nofollow(&current, name).map_err(|error| {
                        format!(
                            "Cannot open mutation parent {} without following links: {error}",
                            relative.display()
                        )
                    })?;
                    (child, metadata, false)
                }
                None if create => match create_directory_entry(
                    &current,
                    name,
                    #[cfg(windows)]
                    policy,
                ) {
                    Ok(child) => {
                        let metadata = child.dir_metadata().map_err(|error| {
                            format!(
                                "Cannot inspect new mutation parent {}: {error}",
                                relative.display()
                            )
                        })?;
                        (child, metadata, true)
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let metadata = current.symlink_metadata(name).map_err(|metadata_error| {
                            format!(
                                "Mutation parent {} appeared without usable metadata: {metadata_error}",
                                relative.display()
                            )
                        })?;
                        if metadata.file_type().is_symlink() || !metadata.is_dir() {
                            return Err(format!(
                                "Refusing symlink or non-directory mutation parent: {}",
                                relative.display()
                            ));
                        }
                        let child = open_directory_nofollow(&current, name).map_err(|open_error| {
                            format!(
                                "Cannot open mutation parent {} without following links: {open_error}",
                                relative.display()
                            )
                        })?;
                        (child, metadata, false)
                    }
                    Err(error) => {
                        return Err(format!(
                            "Cannot create protected mutation parent {}: {error}",
                            relative.display()
                        ));
                    }
                },
                None => {
                    return Err(format!(
                        "Mutation parent {} does not exist",
                        relative.display()
                    ));
                }
            };
            let verification = (|| {
                check_transition(
                    &current,
                    &metadata,
                    #[cfg(windows)]
                    policy,
                    #[cfg(windows)]
                    windows_acl::DaclScope::ProtectedObject,
                )?;
                verify_directory(
                    &child,
                    &metadata,
                    #[cfg(windows)]
                    policy,
                    #[cfg(windows)]
                    if created_here {
                        windows_acl::DaclScope::CreatedObject
                    } else {
                        windows_acl::DaclScope::ProtectedObject
                    },
                )
            })();
            if let Err(error) = verification {
                let cleanup = if created_here {
                    #[cfg(windows)]
                    {
                        delete_open_handle(child.as_raw_handle())
                            .map_err(|cleanup_error| cleanup_error.to_string())
                    }
                    #[cfg(not(windows))]
                    {
                        drop(child);
                        match current.remove_dir(name) {
                            Ok(()) => Ok(()),
                            Err(cleanup_error)
                                if cleanup_error.kind() == std::io::ErrorKind::NotFound =>
                            {
                                Ok(())
                            }
                            Err(cleanup_error) => Err(cleanup_error.to_string()),
                        }
                    }
                } else {
                    Ok(())
                };
                let message = match cleanup {
                    Ok(()) => error,
                    Err(cleanup_error) => format!(
                        "{error}; cannot remove the untrusted directory created for {}: {cleanup_error}",
                        relative.display()
                    ),
                };
                return Err(message);
            }
            let child = Arc::new(child);
            cache.insert(relative.clone(), Arc::clone(&child));
            current = child;
        }
        Ok(current)
    }
}

#[cfg(windows)]
fn dacl_scope_for_path(
    current_path: &Path,
    selected_root: &Path,
    protected_roots: &[PathBuf],
) -> windows_acl::DaclScope {
    if current_path.starts_with(selected_root)
        || protected_roots
            .iter()
            .any(|protected_root| current_path.starts_with(protected_root))
    {
        windows_acl::DaclScope::ProtectedObject
    } else {
        windows_acl::DaclScope::ExistingPathAncestry
    }
}

fn create_directory_entry(
    parent: &Dir,
    name: &OsStr,
    #[cfg(windows)] policy: &windows_acl::DaclPolicy,
) -> std::io::Result<Dir> {
    #[cfg(windows)]
    {
        create_directory_relative(parent, name, policy)
    }
    #[cfg(unix)]
    {
        let mut builder = cap_std::fs::DirBuilder::new();
        use cap_std::fs::DirBuilderExt;
        builder.mode(0o700);
        parent.create_dir_with(name, &builder)?;
        open_directory_nofollow(parent, name)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (parent, name);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Protected directory creation is unsupported on this platform",
        ))
    }
}

fn create_file_entry(
    parent: &Dir,
    name: &OsStr,
    #[cfg(windows)] policy: &windows_acl::DaclPolicy,
) -> std::io::Result<cap_std::fs::File> {
    #[cfg(windows)]
    {
        create_file_relative(parent, name, policy)
    }
    #[cfg(unix)]
    {
        let mut options = cap_std::fs::OpenOptions::new();
        use cap_std::fs::OpenOptionsExt;
        options
            .read(true)
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No)
            .mode(0o600);
        parent.open_with(name, &options)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (parent, name);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Protected file creation is unsupported on this platform",
        ))
    }
}

fn validated_components(relative: &Path) -> super::Result<Vec<OsString>> {
    if relative.is_absolute() {
        return Err("Mutation path must be relative to its pinned root".into());
    }
    let mut result = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => result.push(value.to_os_string()),
            _ => return Err(format!("Unsafe mutation path: {}", relative.display())),
        }
    }
    Ok(result)
}

fn verify_directory(
    dir: &Dir,
    expected: &cap_std::fs::Metadata,
    #[cfg(windows)] policy: &windows_acl::DaclPolicy,
    #[cfg(windows)] scope: windows_acl::DaclScope,
) -> super::Result<()> {
    let opened = dir.dir_metadata().map_err(|error| error.to_string())?;
    if !opened.is_dir() || opened.file_type().is_symlink() {
        return Err("Opened mutation component is not a regular directory".into());
    }
    #[cfg(windows)]
    if opened.file_attributes()
        & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
        != 0
    {
        return Err("Opened mutation component is a reparse point".into());
    }
    same_file(expected, &opened)?;
    check_directory_metadata(
        &opened,
        #[cfg(windows)]
        policy,
        #[cfg(windows)]
        scope,
        dir,
    )
}

fn check_transition(
    parent: &Dir,
    child: &cap_std::fs::Metadata,
    #[cfg(windows)] policy: &windows_acl::DaclPolicy,
    #[cfg(windows)] scope: windows_acl::DaclScope,
) -> super::Result<()> {
    let parent_metadata = parent.dir_metadata().map_err(|error| error.to_string())?;
    check_directory_metadata(
        &parent_metadata,
        #[cfg(windows)]
        policy,
        #[cfg(windows)]
        scope,
        parent,
    )?;
    #[cfg(target_os = "linux")]
    {
        use cap_fs_ext::OsMetadataExt;
        let mode = parent_metadata.mode() & 0o7777;
        if mode & 0o022 != 0
            && mode & 0o1000 != 0
            && child.uid() != rustix::process::geteuid().as_raw()
        {
            return Err(
                "Writable sticky ancestry is safe only when its child belongs to the current user"
                    .into(),
            );
        }
    }
    #[cfg(windows)]
    let _ = (child, policy, scope);
    #[cfg(not(any(target_os = "linux", windows)))]
    let _ = child;
    Ok(())
}

fn check_directory_metadata(
    dir_metadata: &cap_std::fs::Metadata,
    #[cfg(windows)] policy: &windows_acl::DaclPolicy,
    #[cfg(windows)] scope: windows_acl::DaclScope,
    dir: &Dir,
) -> super::Result<()> {
    #[cfg(not(windows))]
    let _ = dir;
    if !dir_metadata.is_dir() || dir_metadata.file_type().is_symlink() {
        return Err("Unsafe mutation directory type".into());
    }
    #[cfg(target_os = "linux")]
    {
        use cap_fs_ext::OsMetadataExt;
        let uid = rustix::process::geteuid().as_raw();
        let mode = dir_metadata.mode() & 0o7777;
        if dir_metadata.uid() != uid && dir_metadata.uid() != 0 {
            return Err("Mutation ancestry must be owned by the current user or root".into());
        }
        if mode & 0o022 != 0 && mode & 0o1000 == 0 {
            return Err("Mutation ancestry is writable by other local users".into());
        }
        if mode & 0o022 != 0 && mode & 0o1000 != 0 && dir_metadata.uid() == uid {
            // The sticky bit does not restrict the directory owner. Root-owned
            // sticky ancestors such as /tmp are safe for non-root mutation, but
            // a current-user-owned shared directory lets this process replace
            // entries another local user creates after a separate state check.
            return Err(
                "Writable sticky mutation directories owned by the current user are unsafe".into(),
            );
        }
    }
    #[cfg(windows)]
    policy.check_handle(dir.as_raw_handle(), scope)?;
    Ok(())
}

fn verify_file(
    file: &cap_std::fs::File,
    metadata: &cap_std::fs::Metadata,
    #[cfg(windows)] policy: &windows_acl::DaclPolicy,
) -> super::Result<()> {
    #[cfg(not(windows))]
    let _ = file;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Mutation target is not a regular file".into());
    }
    #[cfg(target_os = "linux")]
    {
        use cap_fs_ext::OsMetadataExt;
        let uid = rustix::process::geteuid().as_raw();
        if metadata.uid() != uid && metadata.uid() != 0 {
            return Err("Mutation files must be owned by the current user or root".into());
        }
        if metadata.mode() & 0o022 != 0 {
            return Err("Mutation file is writable by other local users".into());
        }
    }
    #[cfg(windows)]
    policy.check_handle(
        file.as_raw_handle(),
        windows_acl::DaclScope::ProtectedObject,
    )?;
    Ok(())
}

fn verify_created_file(
    file: &cap_std::fs::File,
    metadata: &cap_std::fs::Metadata,
    #[cfg(windows)] policy: &windows_acl::DaclPolicy,
) -> super::Result<()> {
    verify_file(
        file,
        metadata,
        #[cfg(windows)]
        policy,
    )?;
    #[cfg(windows)]
    policy.check_handle(file.as_raw_handle(), windows_acl::DaclScope::CreatedObject)?;
    Ok(())
}

fn same_file(left: &cap_std::fs::Metadata, right: &cap_std::fs::Metadata) -> super::Result<()> {
    #[cfg(unix)]
    {
        if left.dev() != right.dev() || left.ino() != right.ino() {
            return Err("Mutation path changed while it was being opened".into());
        }
    }
    #[cfg(windows)]
    {
        // Windows ancestry identity is protected by handle-relative no-follow
        // opens, no-delete-sharing directory handles, and the DACL guard.
        // Compare kinds here; stable IDs for the target and Git marker are
        // obtained separately through GetFileInformationByHandle.
        if left.file_type().is_symlink()
            || right.file_type().is_symlink()
            || left.is_dir() != right.is_dir()
            || left.is_file() != right.is_file()
        {
            return Err("Mutation path changed while it was being opened".into());
        }
    }
    Ok(())
}

fn journal_permissions(metadata: &cap_std::fs::Metadata) -> JournalPermissions {
    #[cfg(unix)]
    {
        let mode = metadata.mode() & 0o7777;
        JournalPermissions {
            readonly: mode & 0o222 == 0,
            unix_mode: Some(mode),
        }
    }
    #[cfg(not(unix))]
    JournalPermissions {
        readonly: metadata.permissions().readonly(),
        unix_mode: None,
    }
}

fn create_temp(
    parent: &PinnedParent,
    contents: &[u8],
    #[cfg(windows)] policy: &windows_acl::DaclPolicy,
) -> super::Result<OsString> {
    for _ in 0..128 {
        let counter = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let name = OsString::from(format!(
            ".toe-tickling-adoption-{}-{counter}.tmp",
            std::process::id()
        ));
        match create_file_entry(
            &parent.directory,
            &name,
            #[cfg(windows)]
            policy,
        ) {
            Ok(mut file) => {
                let verification =
                    file.metadata()
                        .map_err(|error| error.to_string())
                        .and_then(|metadata| {
                            verify_created_file(
                                &file,
                                &metadata,
                                #[cfg(windows)]
                                policy,
                            )
                        });
                if let Err(error) = verification {
                    let cleanup = cleanup_created_file(
                        &parent.directory,
                        &name,
                        &file,
                        #[cfg(windows)]
                        policy,
                    );
                    drop(file);
                    return Err(match cleanup {
                        Ok(()) => format!("Cannot safely use temporary adoption journal: {error}"),
                        Err(cleanup_error) => format!(
                            "Cannot safely use temporary adoption journal: {error}; cleanup also failed: {cleanup_error}"
                        ),
                    });
                }
                if let Err(error) = file.write_all(contents).and_then(|()| file.sync_all()) {
                    let cleanup = cleanup_created_file(
                        &parent.directory,
                        &name,
                        &file,
                        #[cfg(windows)]
                        policy,
                    );
                    drop(file);
                    return Err(match cleanup {
                        Ok(()) => format!("Cannot persist temporary adoption journal: {error}"),
                        Err(cleanup_error) => format!(
                            "Cannot persist temporary adoption journal: {error}; cleanup also failed: {cleanup_error}"
                        ),
                    });
                }
                drop(file);
                return Ok(name);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("Cannot create temporary adoption journal: {error}"));
            }
        }
    }
    Err("Cannot allocate a unique adoption temporary name".into())
}

#[cfg(windows)]
mod windows_acl {
    use std::{
        ffi::c_void,
        os::windows::io::RawHandle,
        path::{Component, Path, PathBuf},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, HANDLE, LocalFree},
        Security::Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            ConvertStringSidToSidW, GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        Security::{
            ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, CreateWellKnownSid,
            DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation, GetLengthSid,
            GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetTokenInformation,
            IsValidSid, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED,
            SECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER, TokenUser, WELL_KNOWN_SID_TYPE,
            WinBuiltinAdministratorsSid, WinCreatorOwnerSid, WinLocalSystemSid,
        },
        Storage::FileSystem::{
            FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_APPEND_DATA, FILE_DELETE_CHILD,
            FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, GetDriveTypeW,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
        System::WindowsProgramming::DRIVE_FIXED,
    };

    const WRITE_DAC: u32 = 0x0004_0000;
    const WRITE_OWNER: u32 = 0x0008_0000;
    const DELETE: u32 = 0x0001_0000;
    const ACCESS_SYSTEM_SECURITY: u32 = 0x0100_0000;
    const MAXIMUM_ALLOWED: u32 = 0x0200_0000;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_EXECUTE: u32 = 0x2000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const GENERIC_ALL: u32 = 0x1000_0000;
    const INHERIT_ONLY_ACE_FLAG: u8 = 0x08;
    const GENERIC_RIGHTS: u32 = GENERIC_READ | GENERIC_WRITE | GENERIC_EXECUTE | GENERIC_ALL;
    const KNOWN_FILE_ACCESS_RIGHTS: u32 =
        0x000f_01ff | 0x0010_0000 | ACCESS_SYSTEM_SECURITY | MAXIMUM_ALLOWED | GENERIC_RIGHTS;
    const CHILD_CREATION_RIGHTS: u32 = FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum DaclScope {
        ExistingPathAncestry,
        ProtectedObject,
        CreatedObject,
    }

    pub(super) struct DaclPolicy {
        trusted_sids: Vec<Vec<u8>>,
        current_user_sid: Vec<u8>,
        owner_rights_sid: Vec<u8>,
    }

    pub(super) struct CreationSecurityDescriptor {
        raw: PSECURITY_DESCRIPTOR,
    }

    impl CreationSecurityDescriptor {
        pub(super) fn as_ptr(&self) -> *const SECURITY_DESCRIPTOR {
            self.raw.cast()
        }
    }

    impl Drop for CreationSecurityDescriptor {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.raw.cast());
            }
        }
    }

    impl DaclPolicy {
        pub(super) fn current() -> super::super::super::Result<Self> {
            let current = current_user_sid()?;
            let system = well_known_sid(WinLocalSystemSid)?;
            let admins = well_known_sid(WinBuiltinAdministratorsSid)?;
            let creator_owner = well_known_sid(WinCreatorOwnerSid)?;
            let trusted_installer =
                string_sid("S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464")?;
            let owner_rights_sid = string_sid("S-1-3-4")?;
            Ok(Self {
                trusted_sids: vec![
                    current.clone(),
                    system,
                    admins,
                    creator_owner,
                    trusted_installer,
                ],
                current_user_sid: current,
                owner_rights_sid,
            })
        }

        pub(super) fn creation_security_descriptor(
            &self,
        ) -> super::super::super::Result<CreationSecurityDescriptor> {
            let current_sid = sid_string(self.current_user_sid.as_ptr() as PSID);
            if current_sid == "unavailable" {
                return Err(
                    "Cannot format the current Windows user SID for a new object's DACL".into(),
                );
            }
            let sddl = format!("O:{current_sid}D:P(A;;FA;;;{current_sid})(A;;FA;;;SY)(A;;FA;;;BA)");
            let wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
            let mut raw = std::ptr::null_mut();
            let mut descriptor_size = 0u32;
            let converted = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    wide.as_ptr(),
                    SDDL_REVISION_1,
                    &mut raw,
                    &mut descriptor_size,
                )
            };
            if converted == 0 || raw.is_null() || descriptor_size == 0 {
                if !raw.is_null() {
                    unsafe { LocalFree(raw.cast()) };
                }
                return Err(format!(
                    "Cannot build a protected Windows creation DACL (error {})",
                    unsafe { GetLastError() }
                )
                .into());
            }
            Ok(CreationSecurityDescriptor { raw })
        }

        pub(super) fn check_handle(
            &self,
            raw: RawHandle,
            scope: DaclScope,
        ) -> super::super::super::Result<()> {
            let handle = raw as HANDLE;
            let mut owner: PSID = std::ptr::null_mut();
            let mut dacl: *mut ACL = std::ptr::null_mut();
            let mut descriptor = std::ptr::null_mut();
            let status = unsafe {
                GetSecurityInfo(
                    handle,
                    SE_FILE_OBJECT,
                    OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                    &mut owner,
                    std::ptr::null_mut(),
                    &mut dacl,
                    std::ptr::null_mut(),
                    &mut descriptor,
                )
            };
            if status != 0 || descriptor.is_null() {
                if !descriptor.is_null() {
                    unsafe { LocalFree(descriptor) };
                }
                return Err(format!(
                    "Cannot read a mutation object's DACL by handle (Windows error {status})"
                )
                .into());
            }
            let result = unsafe { self.check_descriptor(owner, dacl, descriptor, scope) };
            unsafe {
                LocalFree(descriptor);
            }
            result
        }

        unsafe fn check_descriptor(
            &self,
            owner: PSID,
            dacl: *mut ACL,
            descriptor: *mut c_void,
            scope: DaclScope,
        ) -> super::super::super::Result<()> {
            if owner.is_null() || unsafe { IsValidSid(owner) } == 0 || !self.trusted(owner) {
                return Err("Mutation object's owner is not trusted".into());
            }
            if scope == DaclScope::CreatedObject {
                let mut control = 0u16;
                let mut revision = 0u32;
                if unsafe {
                    GetSecurityDescriptorControl(descriptor.cast(), &mut control, &mut revision)
                } == 0
                    || control & SE_DACL_PROTECTED == 0
                {
                    return Err("New mutation object does not have a protected DACL".into());
                }
            }
            let mut present = 0;
            let mut defaulted = 0;
            let mut checked_dacl = std::ptr::null_mut();
            if unsafe {
                GetSecurityDescriptorDacl(
                    descriptor,
                    &mut present,
                    &mut checked_dacl,
                    &mut defaulted,
                )
            } == 0
                || present == 0
                || checked_dacl.is_null()
                || checked_dacl != dacl
            {
                return Err("Mutation object has a null, unreadable, or ambiguous DACL".into());
            }
            let mut size = ACL_SIZE_INFORMATION::default();
            if unsafe {
                GetAclInformation(
                    checked_dacl,
                    &mut size as *mut _ as *mut c_void,
                    std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                )
            } == 0
            {
                return Err("Cannot parse mutation object's DACL".into());
            }
            let acl_size = unsafe { (*checked_dacl).AclSize } as u32;
            if size.AclBytesInUse > acl_size {
                return Err("Mutation object's DACL length is inconsistent".into());
            }
            let acl_start = checked_dacl as usize;
            let acl_end = acl_start
                .checked_add(size.AclBytesInUse as usize)
                .ok_or("Mutation object's DACL length overflowed")?;
            for index in 0..size.AceCount {
                let mut ace_pointer = std::ptr::null_mut();
                if unsafe { GetAce(checked_dacl, index, &mut ace_pointer) } == 0
                    || ace_pointer.is_null()
                {
                    return Err("Cannot read mutation object's DACL entry".into());
                }
                let header = unsafe { &*(ace_pointer as *const ACE_HEADER) };
                if scope == DaclScope::CreatedObject && header.AceFlags != 0 {
                    return Err(
                        "New mutation object's DACL contains an inherited or inheritable ACE"
                            .into(),
                    );
                }
                let ace_start = ace_pointer as usize;
                let ace_size = header.AceSize as usize;
                let ace_end = ace_start
                    .checked_add(ace_size)
                    .ok_or("Mutation object's DACL entry length overflowed")?;
                let first_ace = acl_start
                    .checked_add(std::mem::size_of::<ACL>())
                    .ok_or("Mutation object's DACL length overflowed")?;
                if ace_start < first_ace
                    || ace_end > acl_end
                    || ace_size < std::mem::size_of::<ACE_HEADER>() + std::mem::size_of::<u32>()
                {
                    return Err("Mutation object's DACL contains a truncated ACE".into());
                }
                let mask = unsafe { *((ace_pointer as *const u8).add(4) as *const u32) };
                if !access_mask_is_supported(mask) {
                    return Err(format!(
                        "Mutation object's DACL contains maximum-allowed or unknown access rights (mask 0x{mask:08x})"
                    ));
                }
                if header.AceType == 1 {
                    // ACCESS_DENIED_ACE_TYPE
                    continue;
                }
                if header.AceType == 0 {
                    // ACCESS_ALLOWED_ACE_TYPE
                    let sid = unsafe { (ace_pointer as *const u8).add(8) as PSID };
                    let sid_capacity = ace_size - 8;
                    if sid_capacity < 8 {
                        return Err("Mutation object's DACL contains a truncated allow ACE".into());
                    }
                    let sub_authorities = unsafe { *((sid as *const u8).add(1)) } as usize;
                    let sid_size = 8usize
                        .checked_add(
                            sub_authorities
                                .checked_mul(4)
                                .ok_or("Mutation object's DACL SID length overflowed")?,
                        )
                        .ok_or("Mutation object's DACL SID length overflowed")?;
                    if sid_size > sid_capacity
                        || unsafe { IsValidSid(sid) } == 0
                        || unsafe { GetLengthSid(sid) as usize } != sid_size
                    {
                        return Err("Mutation object's DACL contains an invalid allow SID".into());
                    }
                    if !allow_ace_grant_is_permitted(
                        mask,
                        scope,
                        self.trusted_for_object(sid, owner),
                        header.AceFlags,
                    ) {
                        return Err(format!(
                            "Mutation object's DACL grants an untrusted trustee mutation rights (mask 0x{:08x}, flags 0x{:02x}, SID {})",
                            mask & mutation_rights(),
                            header.AceFlags,
                            sid_string(sid)
                        ));
                    }
                    continue;
                }
                return Err("Mutation object's DACL contains an unsupported ACE type".into());
            }
            Ok(())
        }

        fn trusted(&self, sid: PSID) -> bool {
            self.trusted_sids
                .iter()
                .any(|candidate| unsafe { EqualSid(sid, candidate.as_ptr() as PSID) != 0 })
        }

        fn trusted_for_object(&self, sid: PSID, owner: PSID) -> bool {
            self.trusted(sid)
                || (self.trusted(owner)
                    && unsafe { EqualSid(sid, self.owner_rights_sid.as_ptr() as PSID) != 0 })
        }
    }

    fn mutation_rights() -> u32 {
        FILE_WRITE_DATA
            | FILE_APPEND_DATA
            | FILE_WRITE_EA
            | FILE_WRITE_ATTRIBUTES
            | FILE_ADD_FILE
            | FILE_ADD_SUBDIRECTORY
            | FILE_DELETE_CHILD
            | DELETE
            | WRITE_DAC
            | WRITE_OWNER
            | ACCESS_SYSTEM_SECURITY
            | GENERIC_WRITE
            | GENERIC_ALL
    }

    fn access_mask_is_supported(mask: u32) -> bool {
        mask & MAXIMUM_ALLOWED == 0 && mask & !KNOWN_FILE_ACCESS_RIGHTS == 0
    }

    fn allow_ace_grant_is_permitted(
        mask: u32,
        scope: DaclScope,
        trusted: bool,
        ace_flags: u8,
    ) -> bool {
        // Inherit-only grants do not apply to this object. The handle-anchored
        // path walk checks each descendant's own DACL, where inherited grants
        // become effective and must pass this same policy.
        trusted
            || ace_flags & INHERIT_ONLY_ACE_FLAG != 0
            || untrusted_mutation_grant_allowed(mask, scope)
    }

    fn untrusted_mutation_grant_allowed(mask: u32, scope: DaclScope) -> bool {
        let mutation_grant = mask & mutation_rights();
        mutation_grant == 0
            || (scope == DaclScope::ExistingPathAncestry
                && mutation_grant & !CHILD_CREATION_RIGHTS == 0)
    }

    fn current_user_sid() -> super::super::super::Result<Vec<u8>> {
        let mut token: HANDLE = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err("Cannot open the current Windows security token".into());
        }
        let mut length = 0;
        unsafe {
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut length);
        }
        if (length as usize) < std::mem::size_of::<TOKEN_USER>() {
            unsafe {
                CloseHandle(token);
            }
            return Err("Cannot size the current Windows user SID".into());
        }
        let words = (length as usize).div_ceil(std::mem::size_of::<usize>());
        let mut buffer = vec![0usize; words];
        let mut returned_length = length;
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast::<c_void>(),
                length,
                &mut returned_length,
            )
        };
        unsafe {
            CloseHandle(token);
        }
        if ok == 0 || (returned_length as usize) < std::mem::size_of::<TOKEN_USER>() {
            return Err("Cannot read the current Windows user SID".into());
        }
        let user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
        copy_sid(user.User.Sid)
    }

    fn well_known_sid(kind: WELL_KNOWN_SID_TYPE) -> super::super::super::Result<Vec<u8>> {
        let mut sid = vec![0u8; 68];
        let mut length = sid.len() as u32;
        if unsafe {
            CreateWellKnownSid(
                kind,
                std::ptr::null_mut(),
                sid.as_mut_ptr() as PSID,
                &mut length,
            )
        } == 0
        {
            return Err("Cannot construct a trusted Windows SID".into());
        }
        sid.truncate(length as usize);
        Ok(sid)
    }

    fn string_sid(value: &str) -> super::super::super::Result<Vec<u8>> {
        let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let mut sid: PSID = std::ptr::null_mut();
        if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut sid) } == 0 || sid.is_null() {
            return Err("Cannot construct the TrustedInstaller SID".into());
        }
        let bytes = copy_sid(sid);
        unsafe {
            LocalFree(sid);
        }
        bytes
    }

    fn sid_string(sid: PSID) -> String {
        let mut wide = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(sid, &mut wide) } == 0 || wide.is_null() {
            return "unavailable".into();
        }
        let mut length = 0usize;
        while unsafe { *wide.add(length) } != 0 {
            length += 1;
        }
        let value = unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(wide, length)) };
        unsafe {
            LocalFree(wide.cast());
        }
        value
    }

    fn copy_sid(sid: PSID) -> super::super::super::Result<Vec<u8>> {
        if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
            return Err("Windows returned an invalid SID".into());
        }
        let length = unsafe { GetLengthSid(sid) } as usize;
        if length < 8 || length > 68 {
            return Err("Windows returned an unexpected SID length".into());
        }
        Ok(unsafe { std::slice::from_raw_parts(sid as *const u8, length) }.to_vec())
    }

    pub(super) fn local_volume_root(path: &Path) -> super::super::super::Result<PathBuf> {
        let prefix = path
            .components()
            .next()
            .ok_or("Missing Windows volume prefix")?;
        let Component::Prefix(prefix) = prefix else {
            return Err("Mutation path must have a drive-letter volume prefix".into());
        };
        let text = prefix.as_os_str().to_string_lossy();
        if !(text.len() == 2 && text.as_bytes()[1] == b':'
            || text.starts_with("\\\\?\\") && text.len() == 6 && text.as_bytes()[5] == b':')
        {
            return Err("UNC, device, and non-drive Windows paths are paused".into());
        }
        let mut root = prefix.as_os_str().to_os_string();
        root.push("\\");
        let root = PathBuf::from(root);
        let drive = if text.starts_with("\\\\?\\") {
            &text[4..5]
        } else {
            &text[..1]
        };
        let drive_root: Vec<u16> = format!("{drive}:\\")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        if unsafe { GetDriveTypeW(drive_root.as_ptr()) } != DRIVE_FIXED {
            return Err(
                "Guidebook adoption mutations require a verified local fixed volume".into(),
            );
        }
        Ok(root)
    }

    #[cfg(test)]
    mod tests {
        use super::string_sid;
        use super::{
            ACCESS_SYSTEM_SECURITY, DELETE, DaclPolicy, DaclScope, GENERIC_ALL, GENERIC_EXECUTE,
            GENERIC_READ, GENERIC_WRITE, INHERIT_ONLY_ACE_FLAG, MAXIMUM_ALLOWED, WRITE_DAC,
            WRITE_OWNER, access_mask_is_supported, allow_ace_grant_is_permitted, mutation_rights,
            sid_string, untrusted_mutation_grant_allowed, well_known_sid,
        };
        use windows_sys::Win32::Security::{PSID, WinBuiltinUsersSid};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_APPEND_DATA, FILE_DELETE_CHILD,
            FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA,
        };

        #[test]
        fn mutation_mask_covers_file_and_directory_replace_rights() {
            for right in [
                FILE_WRITE_DATA,
                FILE_ADD_FILE,
                FILE_ADD_SUBDIRECTORY,
                FILE_DELETE_CHILD,
            ] {
                assert_ne!(mutation_rights() & right, 0);
            }
        }

        #[test]
        fn child_creation_is_allowed_only_on_existing_path_ancestry() {
            // Windows reuses these access-mask bits by object type: on an
            // ancestry directory they mean child creation, not file writes.
            assert_eq!(FILE_ADD_FILE, FILE_WRITE_DATA);
            assert_eq!(FILE_ADD_SUBDIRECTORY, FILE_APPEND_DATA);
            for creation_right in [FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY] {
                assert!(untrusted_mutation_grant_allowed(
                    creation_right,
                    DaclScope::ExistingPathAncestry
                ));
                assert!(!untrusted_mutation_grant_allowed(
                    creation_right,
                    DaclScope::ProtectedObject
                ));
                for forbidden_right in [
                    FILE_DELETE_CHILD,
                    DELETE,
                    FILE_WRITE_EA,
                    FILE_WRITE_ATTRIBUTES,
                    WRITE_DAC,
                    WRITE_OWNER,
                    ACCESS_SYSTEM_SECURITY,
                ] {
                    assert!(!untrusted_mutation_grant_allowed(
                        creation_right | forbidden_right,
                        DaclScope::ExistingPathAncestry
                    ));
                }
            }
            for file_write_right in [FILE_WRITE_DATA, FILE_APPEND_DATA] {
                assert!(!untrusted_mutation_grant_allowed(
                    file_write_right,
                    DaclScope::ProtectedObject
                ));
            }
        }

        #[test]
        fn generic_read_and_execute_are_non_mutating() {
            for read_or_execute in [
                GENERIC_READ,
                GENERIC_EXECUTE,
                GENERIC_READ | GENERIC_EXECUTE,
            ] {
                assert!(access_mask_is_supported(read_or_execute));
                assert!(untrusted_mutation_grant_allowed(
                    read_or_execute,
                    DaclScope::ExistingPathAncestry
                ));
                assert!(untrusted_mutation_grant_allowed(
                    read_or_execute,
                    DaclScope::ProtectedObject
                ));
            }
            assert!(access_mask_is_supported(GENERIC_READ | 0x0000_0001));
            assert!(access_mask_is_supported(0x0000_0001));
            assert!(access_mask_is_supported(FILE_ADD_SUBDIRECTORY));
        }

        #[test]
        fn generic_write_and_all_are_only_permitted_for_trusted_trustees() {
            for generic_write in [
                GENERIC_WRITE,
                GENERIC_ALL,
                GENERIC_READ | GENERIC_WRITE,
                GENERIC_READ | GENERIC_WRITE | GENERIC_EXECUTE | 0x0010_0000,
            ] {
                assert!(access_mask_is_supported(generic_write));
                for scope in [DaclScope::ExistingPathAncestry, DaclScope::ProtectedObject] {
                    assert!(!allow_ace_grant_is_permitted(
                        generic_write,
                        scope,
                        false,
                        0
                    ));
                    assert!(allow_ace_grant_is_permitted(generic_write, scope, true, 0));
                }
            }
        }

        #[test]
        fn inherit_only_write_grant_is_ignored_only_on_the_attached_object() {
            let generic_write = GENERIC_WRITE | 0x0010_0000;

            assert!(allow_ace_grant_is_permitted(
                generic_write,
                DaclScope::ProtectedObject,
                false,
                INHERIT_ONLY_ACE_FLAG | 0x03,
            ));
            assert!(!allow_ace_grant_is_permitted(
                generic_write,
                DaclScope::ProtectedObject,
                false,
                0x03,
            ));
        }

        #[test]
        fn owner_rights_ace_is_trusted_only_when_object_owner_is_trusted() {
            let policy = DaclPolicy::current().unwrap();
            let owner_rights = string_sid("S-1-3-4").unwrap();
            let owner_rights_sid = owner_rights.as_ptr() as PSID;
            let trusted_owner = policy.trusted_sids[0].as_ptr() as PSID;
            let local_users = well_known_sid(WinBuiltinUsersSid).unwrap();

            assert!(policy.trusted_for_object(owner_rights_sid, trusted_owner));
            assert!(!policy.trusted_for_object(owner_rights_sid, local_users.as_ptr() as PSID));
        }

        #[test]
        fn sid_diagnostics_use_the_windows_string_representation() {
            let owner_rights = string_sid("S-1-3-4").unwrap();

            assert_eq!(sid_string(owner_rights.as_ptr() as PSID), "S-1-3-4");
        }

        #[test]
        fn maximum_allowed_and_unknown_access_masks_are_rejected() {
            for unsupported in [MAXIMUM_ALLOWED, 0x0080_0000] {
                assert!(!access_mask_is_supported(unsupported));
            }
        }

        #[test]
        fn git_metadata_keeps_shared_target_root_strict() {
            let volume = std::path::PathBuf::from(r"C:\");
            let target = volume.join(r"Users\runner\work\target");
            let git = target.join(".git");
            let protected_roots = vec![target.clone()];

            assert_eq!(
                super::super::dacl_scope_for_path(&volume, &git, &protected_roots),
                DaclScope::ExistingPathAncestry
            );
            assert_eq!(
                super::super::dacl_scope_for_path(&target, &git, &protected_roots),
                DaclScope::ProtectedObject
            );
            assert_eq!(
                super::super::dacl_scope_for_path(&git, &git, &protected_roots),
                DaclScope::ProtectedObject
            );
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::MutationContext;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::Path,
        sync::{Arc, Barrier},
        thread,
    };

    fn target_and_git(work: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let target = work.join("target");
        let git = target.join(".git");
        fs::create_dir_all(&git).unwrap();
        (target.canonicalize().unwrap(), git.canonicalize().unwrap())
    }

    #[test]
    fn root_is_rejected_before_mutation_paths_are_opened() {
        let result = MutationContext::ensure_supported();
        if rustix::process::geteuid().as_raw() == 0 {
            assert!(result.expect_err("root must be rejected").contains("root"));
        } else {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn writable_nonsticky_ancestry_is_rejected_without_touching_target() {
        if rustix::process::geteuid().as_raw() == 0 {
            // Root is rejected even earlier by policy; CI's unprivileged job
            // exercises the writable-ancestor check below.
            return;
        }
        let work = tempfile::tempdir().unwrap();
        let unsafe_parent = work.path().join("writable-parent");
        fs::create_dir(&unsafe_parent).unwrap();
        fs::set_permissions(&unsafe_parent, fs::Permissions::from_mode(0o777)).unwrap();
        let (target, git) = target_and_git(&unsafe_parent);
        let sentinel = target.join("sentinel");
        fs::write(&sentinel, b"unchanged").unwrap();

        let error = match MutationContext::open(&target, &git) {
            Ok(_) => panic!("writable ancestry must be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("writable"), "unexpected error: {error}");
        assert_eq!(fs::read(sentinel).unwrap(), b"unchanged");
    }

    #[test]
    fn current_user_owned_writable_sticky_target_is_rejected_during_competing_create() {
        if rustix::process::geteuid().as_raw() == 0 {
            return;
        }
        let work = tempfile::tempdir().unwrap();
        let (target, git) = target_and_git(work.path());
        fs::set_permissions(&target, fs::Permissions::from_mode(0o1777)).unwrap();
        let sentinel = target.join("sentinel");
        fs::write(&sentinel, b"unchanged").unwrap();
        let competing_entry = target.join("competing-entry");
        let start = Arc::new(Barrier::new(2));
        let competitor_start = Arc::clone(&start);
        let competitor = thread::spawn(move || {
            competitor_start.wait();
            fs::write(competing_entry, b"synthetic competing entry").unwrap();
        });

        start.wait();
        let result = MutationContext::open(&target, &git);
        competitor.join().unwrap();
        let error = match result {
            Ok(_) => panic!("a current-user-owned writable sticky target must be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("sticky"), "unexpected error: {error}");
        assert_eq!(fs::read(sentinel).unwrap(), b"unchanged");
        assert_eq!(
            fs::read(target.join("competing-entry")).unwrap(),
            b"synthetic competing entry"
        );
        assert!(!target.join(".ai-guidebook").exists());
    }

    #[test]
    fn protected_target_allows_handle_anchored_writes() {
        if rustix::process::geteuid().as_raw() == 0 {
            return;
        }
        let work = tempfile::tempdir().unwrap();
        let (target, git) = target_and_git(work.path());
        let context = MutationContext::open(&target, &git).unwrap();
        let output = target.join(".ai-guidebook/policy-lock.json");

        context.write_new(&output, b"protected output").unwrap();

        assert_eq!(fs::read(output).unwrap(), b"protected output");
    }

    #[test]
    fn pinned_directory_write_cannot_follow_a_replacement_symlink() {
        if rustix::process::geteuid().as_raw() == 0 {
            return;
        }
        let work = tempfile::tempdir().unwrap();
        let (target, git) = target_and_git(work.path());
        let pinned = target.join("pinned");
        let moved = target.join("pinned-moved");
        let outside = work.path().join("outside");
        fs::create_dir(&pinned).unwrap();
        fs::create_dir(&outside).unwrap();
        let sentinel = outside.join("sentinel");
        fs::write(&sentinel, b"outside bytes").unwrap();

        let context = MutationContext::open(&target, &git).unwrap();
        context.pin_directory(&pinned).unwrap();
        fs::rename(&pinned, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &pinned).unwrap();

        context
            .write_new(
                &pinned.join("created-by-pinned-handle"),
                b"inside pinned directory",
            )
            .unwrap();

        assert_eq!(
            fs::read(moved.join("created-by-pinned-handle")).unwrap(),
            b"inside pinned directory"
        );
        assert!(!outside.join("created-by-pinned-handle").exists());
        assert_eq!(fs::read(sentinel).unwrap(), b"outside bytes");
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::{MutationContext, open_mutation_file};
    use std::{
        ffi::OsStr,
        fs, io,
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
        path::Path,
        process::Command,
        sync::mpsc::{self, Receiver, SyncSender},
        thread::{self, JoinHandle},
        time::Duration,
    };

    fn open_directory_with_add_file(
        path: &Path,
        policy: &super::windows_acl::DaclPolicy,
    ) -> io::Result<cap_std::fs::Dir> {
        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            Storage::FileSystem::{
                CreateFileW, FILE_ADD_FILE, FILE_FLAG_BACKUP_SEMANTICS,
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, OPEN_EXISTING, READ_CONTROL,
                SYNCHRONIZE,
            },
        };

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_ADD_FILE | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let directory =
            cap_std::fs::Dir::from(unsafe { OwnedHandle::from_raw_handle(handle as _) });
        policy
            .check_handle(
                directory.as_raw_handle(),
                super::windows_acl::DaclScope::ProtectedObject,
            )
            .map_err(|error| io::Error::other(error.to_string()))?;
        Ok(directory)
    }

    fn directory_id(directory: &cap_std::fs::Dir) -> (u64, [u8; 16]) {
        use windows_sys::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx},
        };

        let mut info = FILE_ID_INFO::default();
        let succeeded = unsafe {
            GetFileInformationByHandleEx(
                directory.as_raw_handle() as HANDLE,
                FileIdInfo,
                (&mut info as *mut FILE_ID_INFO).cast(),
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
        };
        assert_ne!(
            succeeded, 0,
            "could not identify diagnostic directory handle"
        );
        (info.VolumeSerialNumber, info.FileId.Identifier)
    }

    fn set_file_rename_info(
        file: &cap_std::fs::File,
        root_directory: Option<&cap_std::fs::Dir>,
        destination: &OsStr,
        extended: bool,
    ) -> io::Result<()> {
        use windows_sys::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                FILE_RENAME_INFO, FILE_RENAME_INFO_0, FileRenameInfo, FileRenameInfoEx,
                SetFileInformationByHandle,
            },
        };

        let wide: Vec<u16> = destination.encode_wide().collect();
        if wide.is_empty() || wide.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "diagnostic destination is empty or contains NUL",
            ));
        }
        let name_length = wide
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .and_then(|length| u32::try_from(length).ok())
            .ok_or_else(|| io::Error::other("diagnostic destination length overflowed"))?;
        let file_name_bytes = wide
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .ok_or_else(|| io::Error::other("diagnostic filename length overflowed"))?;
        let buffer_length = std::mem::size_of::<FILE_RENAME_INFO>()
            .checked_add(file_name_bytes)
            .ok_or_else(|| io::Error::other("diagnostic rename buffer length overflowed"))?;
        let buffer_length_u32 = u32::try_from(buffer_length)
            .map_err(|_| io::Error::other("diagnostic rename buffer exceeds Windows limits"))?;
        let words = buffer_length.div_ceil(std::mem::size_of::<usize>());
        let mut buffer = vec![0usize; words];
        let information = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        let root_directory = root_directory.map_or(std::ptr::null_mut(), |directory| {
            directory.as_raw_handle() as HANDLE
        });
        unsafe {
            (*information).Anonymous = if extended {
                FILE_RENAME_INFO_0 { Flags: 0 }
            } else {
                FILE_RENAME_INFO_0 {
                    ReplaceIfExists: false,
                }
            };
            (*information).RootDirectory = root_directory;
            (*information).FileNameLength = name_length;
            std::ptr::copy_nonoverlapping(
                wide.as_ptr(),
                (*information).FileName.as_mut_ptr(),
                wide.len(),
            );
        }
        let information_class = if extended {
            FileRenameInfoEx
        } else {
            FileRenameInfo
        };
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle() as HANDLE,
                information_class,
                information.cast::<std::ffi::c_void>(),
                buffer_length_u32,
            )
        } == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn set_native_file_rename_info(
        file: &cap_std::fs::File,
        root_directory: &cap_std::fs::Dir,
        destination: &OsStr,
    ) -> io::Result<()> {
        use windows_sys::{
            Wdk::Storage::FileSystem::{
                FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0, FileRenameInformation,
                NtSetInformationFile,
            },
            Win32::{
                Foundation::{HANDLE, RtlNtStatusToDosError, STATUS_SUCCESS},
                System::IO::IO_STATUS_BLOCK,
            },
        };

        let wide: Vec<u16> = destination.encode_wide().collect();
        if wide.is_empty() || wide.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "diagnostic destination is empty or contains NUL",
            ));
        }
        let name_length = wide
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .and_then(|length| u32::try_from(length).ok())
            .ok_or_else(|| io::Error::other("diagnostic destination length overflowed"))?;
        let file_name_bytes = wide
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .ok_or_else(|| io::Error::other("diagnostic filename length overflowed"))?;
        let buffer_length = std::mem::size_of::<FILE_RENAME_INFORMATION>()
            .checked_add(file_name_bytes)
            .ok_or_else(|| io::Error::other("diagnostic rename buffer length overflowed"))?;
        let buffer_length_u32 = u32::try_from(buffer_length)
            .map_err(|_| io::Error::other("diagnostic rename buffer exceeds Windows limits"))?;
        let words = buffer_length.div_ceil(std::mem::size_of::<usize>());
        let mut buffer = vec![0usize; words];
        let information = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
        unsafe {
            (*information).Anonymous = FILE_RENAME_INFORMATION_0 {
                ReplaceIfExists: false,
            };
            (*information).RootDirectory = root_directory.as_raw_handle() as HANDLE;
            (*information).FileNameLength = name_length;
            std::ptr::copy_nonoverlapping(
                wide.as_ptr(),
                (*information).FileName.as_mut_ptr(),
                wide.len(),
            );
        }
        let mut status_block = IO_STATUS_BLOCK::default();
        let status = unsafe {
            NtSetInformationFile(
                file.as_raw_handle() as HANDLE,
                &mut status_block,
                information.cast::<std::ffi::c_void>(),
                buffer_length_u32,
                FileRenameInformation,
            )
        };
        if status == STATUS_SUCCESS {
            Ok(())
        } else {
            let code = unsafe { RtlNtStatusToDosError(status) };
            Err(io::Error::from_raw_os_error(code as i32))
        }
    }

    fn attempt_documented_rename(
        context: &MutationContext,
        source_name: &str,
        destination: &Path,
        root_directory: Option<&cap_std::fs::Dir>,
        extended: bool,
    ) -> Result<(), String> {
        context
            .write_new(
                &context.target.path.join(source_name),
                b"synthetic rename bytes",
            )
            .map_err(|error| error.to_string())?;
        let file = open_mutation_file(&context.target.root, OsStr::new(source_name), &context.dacl)
            .map_err(|error| error.to_string())?;
        set_file_rename_info(&file, root_directory, destination.as_os_str(), extended)
            .map_err(|error| error.to_string())
    }

    fn attempt_move_file_ex(
        context: &MutationContext,
        source_name: &str,
        destination_name: &str,
    ) -> Result<(), String> {
        use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

        let source = context.target.path.join(source_name);
        let destination = context.target.path.join(destination_name);
        context
            .write_new(&source, b"synthetic rename bytes")
            .map_err(|error| error.to_string())?;
        let source_wide: Vec<u16> = source
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let destination_wide: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        if unsafe { MoveFileExW(source_wide.as_ptr(), destination_wide.as_ptr(), 0) } == 0 {
            Err(io::Error::last_os_error().to_string())
        } else {
            Ok(())
        }
    }

    fn attempt_native_relative_rename(
        context: &MutationContext,
        source_name: &str,
        destination_name: &str,
        destination_parent: &cap_std::fs::Dir,
    ) -> Result<(), String> {
        context
            .write_new(
                &context.target.path.join(source_name),
                b"synthetic rename bytes",
            )
            .map_err(|error| error.to_string())?;
        let file = open_mutation_file(&context.target.root, OsStr::new(source_name), &context.dacl)
            .map_err(|error| error.to_string())?;
        set_native_file_rename_info(&file, destination_parent, OsStr::new(destination_name))
            .map_err(|error| error.to_string())
    }

    fn mutation_fixture_tempdir() -> tempfile::TempDir {
        let profile = std::env::var_os("USERPROFILE")
            .map(std::path::PathBuf::from)
            .expect("Windows test profile is available");
        tempfile::tempdir_in(profile).unwrap()
    }

    fn target_and_git(work: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let target = work.join("target");
        let git = target.join(".git");
        fs::create_dir_all(&git).unwrap();
        (target.canonicalize().unwrap(), git.canonicalize().unwrap())
    }

    #[test]
    fn protected_user_profile_path_passes_handle_anchored_preflight() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());

        MutationContext::open(&target, &git)
            .unwrap_or_else(|error| panic!("protected profile path was rejected: {error}"));
    }

    fn grant_everyone_modify(path: &Path, inherited: bool) {
        let rights = if inherited {
            "*S-1-1-0:(OI)(CI)M"
        } else {
            "*S-1-1-0:M"
        };
        let result = Command::new("icacls.exe")
            .arg(path)
            .args(["/grant", rights])
            .output()
            .expect("Windows ACL editor is available on the CI runner");
        assert!(
            result.status.success(),
            "icacls failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    fn race_fixture() -> (
        tempfile::TempDir,
        MutationContext,
        std::path::PathBuf,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let attacker = work.path().join("attacker-controlled");
        fs::create_dir(&attacker).unwrap();
        grant_everyone_modify(&attacker, true);
        let sentinel = work.path().join("outside-sentinel.txt");
        fs::write(&sentinel, b"outside bytes").unwrap();
        let context = MutationContext::open(&target, &git).unwrap();
        (
            work,
            context,
            target,
            attacker.canonicalize().unwrap(),
            sentinel,
        )
    }

    fn move_path(source: &Path, destination: &Path, flags: u32) -> Result<(), u32> {
        use windows_sys::Win32::{Foundation::GetLastError, Storage::FileSystem::MoveFileExW};

        let source_wide: Vec<u16> = source
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let destination_wide: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        if unsafe { MoveFileExW(source_wide.as_ptr(), destination_wide.as_ptr(), flags) } == 0 {
            Err(unsafe { GetLastError() })
        } else {
            Ok(())
        }
    }

    fn delete_path(path: &Path) -> Result<(), u32> {
        use windows_sys::Win32::{Foundation::GetLastError, Storage::FileSystem::DeleteFileW};

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        if unsafe { DeleteFileW(wide.as_ptr()) } == 0 {
            Err(unsafe { GetLastError() })
        } else {
            Ok(())
        }
    }

    fn assert_blocked_by_acl_or_share(result: Result<(), u32>, operation: &str) {
        use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION};

        match result {
            Err(code) if code == ERROR_ACCESS_DENIED || code == ERROR_SHARING_VIOLATION => {}
            other => panic!("restricted token unexpectedly performed {operation}: {other:?}"),
        }
    }

    fn control_move_paths(
        attacker: &Path,
        suffix: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let source = attacker.join(format!("control-{suffix}-source.tmp"));
        let destination = attacker.join(format!("control-{suffix}-destination.tmp"));
        fs::write(&source, b"controlled source").unwrap();
        grant_everyone_modify(&source, false);
        (source, destination)
    }

    fn release_race_actor<T>(
        ready: Receiver<()>,
        release: SyncSender<()>,
        actor: JoinHandle<T>,
    ) -> T {
        ready
            .recv_timeout(Duration::from_secs(10))
            .expect("restricted race actor did not reach the synchronized boundary");
        release
            .send(())
            .expect("restricted race actor stopped before the synchronized attempt");
        actor.join().expect("restricted race actor panicked")
    }

    fn grant_everyone_inherit_only_modify(path: &Path) {
        let result = Command::new("icacls.exe")
            .arg(path)
            .args(["/grant", "*S-1-1-0:(OI)(CI)(IO)M"])
            .output()
            .expect("Windows ACL editor is available on the CI runner");
        assert!(
            result.status.success(),
            "icacls failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    fn assert_dacl_is_protected(handle: std::os::windows::io::RawHandle) {
        use windows_sys::Win32::{
            Foundation::{HANDLE, LocalFree},
            Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
            Security::DACL_SECURITY_INFORMATION,
            Security::{GetSecurityDescriptorControl, SE_DACL_PROTECTED},
        };

        let mut descriptor = std::ptr::null_mut();
        let status = unsafe {
            GetSecurityInfo(
                handle as HANDLE,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        assert_eq!(status, 0, "GetSecurityInfo failed with {status}");
        assert!(
            !descriptor.is_null(),
            "GetSecurityInfo returned no descriptor"
        );

        let mut control = 0u16;
        let mut revision = 0u32;
        let read_control =
            unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) };
        unsafe {
            LocalFree(descriptor.cast());
        }
        assert_ne!(
            read_control, 0,
            "cannot read security descriptor control flags"
        );
        assert_ne!(
            control & SE_DACL_PROTECTED,
            0,
            "new object inherited a DACL from its parent"
        );
    }

    fn spawn_restricted_identity<F, T>(operation: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        // This test token retains the current user's SID and adds Everyone as a
        // restricting SID; it models constrained access, not a separate account.
        use windows_sys::Win32::{
            Foundation::HANDLE,
            Security::{
                CreateRestrictedToken, SID_AND_ATTRIBUTES, TOKEN_DUPLICATE, TOKEN_QUERY,
                WinWorldSid,
            },
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        };

        thread::spawn(move || {
            let mut everyone = [0u8; 68];
            let mut everyone_length = everyone.len() as u32;
            assert_ne!(
                unsafe {
                    windows_sys::Win32::Security::CreateWellKnownSid(
                        WinWorldSid,
                        std::ptr::null_mut(),
                        everyone.as_mut_ptr().cast(),
                        &mut everyone_length,
                    )
                },
                0,
                "cannot construct the Everyone SID for the restricted test token"
            );
            let restricting_sid = SID_AND_ATTRIBUTES {
                Sid: everyone.as_mut_ptr().cast(),
                Attributes: 0,
            };

            let mut source_token: HANDLE = std::ptr::null_mut();
            assert_ne!(
                unsafe {
                    OpenProcessToken(
                        GetCurrentProcess(),
                        TOKEN_DUPLICATE | TOKEN_QUERY,
                        &mut source_token,
                    )
                },
                0,
                "cannot open the process token for the restricted identity test"
            );
            let source_token = unsafe { OwnedHandle::from_raw_handle(source_token as _) };

            let mut restricted_token: HANDLE = std::ptr::null_mut();
            assert_ne!(
                unsafe {
                    CreateRestrictedToken(
                        source_token.as_raw_handle() as HANDLE,
                        0,
                        0,
                        std::ptr::null(),
                        0,
                        std::ptr::null(),
                        1,
                        &restricting_sid,
                        &mut restricted_token,
                    )
                },
                0,
                "cannot create the restricted identity test token"
            );
            let restricted_token = unsafe { OwnedHandle::from_raw_handle(restricted_token as _) };

            assert_ne!(
                unsafe {
                    windows_sys::Win32::Security::ImpersonateLoggedOnUser(
                        restricted_token.as_raw_handle() as HANDLE,
                    )
                },
                0,
                "cannot impersonate the restricted identity test token"
            );
            struct RevertToSelf;
            impl Drop for RevertToSelf {
                fn drop(&mut self) {
                    assert_ne!(
                        unsafe { windows_sys::Win32::Security::RevertToSelf() },
                        0,
                        "cannot leave the restricted identity test token"
                    );
                }
            }
            let _revert = RevertToSelf;
            operation()
        })
    }

    fn assert_restricted_identity_cannot_open(path: &Path, desired_access: u32, flags: u32) {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE},
            Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
        };

        let display = path.display().to_string();
        let path = path.to_path_buf();
        let result = spawn_restricted_identity(move || {
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let handle = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    desired_access,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    flags,
                    std::ptr::null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                Err(unsafe { GetLastError() })
            } else {
                unsafe { CloseHandle(handle) };
                Ok(())
            }
        })
        .join()
        .expect("restricted identity test thread panicked");
        assert_eq!(
            result,
            Err(windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED),
            "restricted identity unexpectedly accessed {}",
            display
        );
    }

    fn grant_everyone_add_subdirectory(path: &Path) {
        let result = Command::new("icacls.exe")
            .arg(path)
            .args(["/grant", "*S-1-1-0:(AD)"])
            .output()
            .expect("Windows ACL editor is available on the CI runner");
        assert!(
            result.status.success(),
            "icacls failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    fn pinned_directory_handle_blocks_renaming_its_path() {
        use windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION;

        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let pinned = target.join("pinned");
        let renamed = target.join("renamed");
        fs::create_dir(&pinned).unwrap();
        let context = MutationContext::open(&target, &git).unwrap();
        context.pin_directory(&pinned).unwrap();

        let error = fs::rename(&pinned, &renamed).unwrap_err();

        let rename_was_blocked = matches!(
            error.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Other
        ) || error.raw_os_error() == Some(ERROR_SHARING_VIOLATION as i32);
        assert!(
            rename_was_blocked,
            "unexpected rename error: {error} (kind {:?}, raw code {:?})",
            error.kind(),
            error.raw_os_error()
        );
        assert!(pinned.is_dir());
        assert!(!renamed.exists());
    }

    #[test]
    fn handle_relative_rename_and_delete_preserve_verified_file_state() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let context = MutationContext::open(&target, &git).unwrap();
        let parent = target.join("nested");
        context
            .create_parent_dirs(&parent.join("placeholder"))
            .unwrap();
        let stage = parent.join("stage.tmp");
        let destination = parent.join("destination");
        context
            .write_new(&stage, b"synthetic staged bytes")
            .unwrap();
        let source_state = context.file_state(&stage).unwrap().unwrap();

        context
            .rename(&stage, &destination, &source_state, None)
            .unwrap();

        assert_eq!(
            context.read(&destination).unwrap(),
            b"synthetic staged bytes"
        );
        let published_state = context.file_state(&destination).unwrap().unwrap();
        context.remove_file(&destination, &published_state).unwrap();
        assert!(context.file_state(&destination).unwrap().is_none());
    }

    #[test]
    fn concurrent_untrusted_replacement_and_removal_are_blocked_before_promotion() {
        use windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING;

        let (_work, context, target, attacker, sentinel) = race_fixture();
        let output_dir = target.join(".ai-guidebook");
        context
            .create_parent_dirs(&output_dir.join("placeholder"))
            .unwrap();
        let stage = output_dir.join("race-stage.tmp");
        let destination = output_dir.join("race-destination.txt");
        context.write_new(&stage, b"staged output").unwrap();
        context
            .write_new(&destination, b"original destination")
            .unwrap();
        let attacker_source = attacker.join("replacement-source.tmp");
        fs::write(&attacker_source, b"attacker replacement").unwrap();
        grant_everyone_modify(&attacker_source, false);
        let actor_attacker_source = attacker_source.clone();
        let (control_source, control_destination) = control_move_paths(&attacker, "promote");

        let (ready, actor_ready) = mpsc::channel();
        let (release, actor_release) = mpsc::sync_channel(0);
        let actor_destination = destination.clone();
        let actor = spawn_restricted_identity(move || {
            let control = move_path(&control_source, &control_destination, 0);
            actor_ready.send(()).unwrap();
            actor_release
                .recv_timeout(Duration::from_secs(10))
                .expect("promotion race hook did not release the restricted actor");
            let replacement = move_path(
                &actor_attacker_source,
                &actor_destination,
                MOVEFILE_REPLACE_EXISTING,
            );
            let removal = delete_path(&actor_destination);
            (control, replacement, removal)
        });

        let stage_state = context.file_state(&stage).unwrap().unwrap();
        let destination_state = context.file_state(&destination).unwrap().unwrap();
        let callback_destination = destination.clone();
        let callback_attacker_source = attacker_source.clone();
        let callback_sentinel = sentinel.clone();
        context
            .rename_with_pre_rename_hook(
                &stage,
                &destination,
                &stage_state,
                Some(&destination_state),
                move || {
                    let (control, replacement, removal) = release_race_actor(ready, release, actor);
                    assert_eq!(control, Ok(()), "restricted control rename failed");
                    assert_blocked_by_acl_or_share(replacement, "replace the destination");
                    assert_blocked_by_acl_or_share(removal, "remove the destination");
                    assert_eq!(
                        fs::read(&callback_destination).unwrap(),
                        b"original destination"
                    );
                    assert!(callback_attacker_source.exists());
                    assert_eq!(fs::read(&callback_sentinel).unwrap(), b"outside bytes");
                    Ok(())
                },
            )
            .unwrap();

        assert_eq!(context.read(&destination).unwrap(), b"staged output");
        assert!(context.file_state(&stage).unwrap().is_none());
        assert!(attacker.join("replacement-source.tmp").exists());
        assert_eq!(fs::read(&sentinel).unwrap(), b"outside bytes");
    }

    #[test]
    fn concurrent_untrusted_replacement_and_removal_are_blocked_at_journal_commit() {
        use windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING;

        let (_work, context, _target, attacker, sentinel) = race_fixture();
        let journal = context.git_path().join("adoption-journal.json");
        context.write_new(&journal, b"prepared journal").unwrap();
        let attacker_source = attacker.join("journal-replacement-source.tmp");
        fs::write(&attacker_source, b"attacker journal").unwrap();
        grant_everyone_modify(&attacker_source, false);
        let actor_attacker_source = attacker_source.clone();
        let (control_source, control_destination) = control_move_paths(&attacker, "journal");

        let (ready, actor_ready) = mpsc::channel();
        let (release, actor_release) = mpsc::sync_channel(0);
        let actor_journal = journal.clone();
        let actor = spawn_restricted_identity(move || {
            let control = move_path(&control_source, &control_destination, 0);
            actor_ready.send(()).unwrap();
            actor_release
                .recv_timeout(Duration::from_secs(10))
                .expect("journal race hook did not release the restricted actor");
            let replacement = move_path(
                &actor_attacker_source,
                &actor_journal,
                MOVEFILE_REPLACE_EXISTING,
            );
            let removal = delete_path(&actor_journal);
            (control, replacement, removal)
        });

        let callback_journal = journal.clone();
        let callback_attacker_source = attacker_source.clone();
        let callback_sentinel = sentinel.clone();
        context
            .write_atomic_with_pre_rename_hook(&journal, b"committed journal", false, move || {
                let (control, replacement, removal) = release_race_actor(ready, release, actor);
                assert_eq!(control, Ok(()), "restricted control rename failed");
                assert_blocked_by_acl_or_share(replacement, "replace the journal");
                assert_blocked_by_acl_or_share(removal, "remove the journal");
                assert_eq!(fs::read(&callback_journal).unwrap(), b"prepared journal");
                assert!(callback_attacker_source.exists());
                assert_eq!(fs::read(&callback_sentinel).unwrap(), b"outside bytes");
                Ok(())
            })
            .unwrap();

        assert_eq!(context.read(&journal).unwrap(), b"committed journal");
        assert!(attacker.join("journal-replacement-source.tmp").exists());
        assert_eq!(fs::read(&sentinel).unwrap(), b"outside bytes");
    }

    #[test]
    fn concurrent_untrusted_removal_is_blocked_during_stage_and_journal_cleanup() {
        let (_work, context, target, attacker, sentinel) = race_fixture();
        let stage_dir = target.join(".ai-guidebook");
        context
            .create_parent_dirs(&stage_dir.join("placeholder"))
            .unwrap();
        let cleanup_targets = [
            (
                "stage",
                stage_dir.join("cleanup-stage.tmp"),
                b"stage cleanup".as_slice(),
            ),
            (
                "journal",
                context.git_path().join("cleanup-journal.tmp"),
                b"journal cleanup".as_slice(),
            ),
        ];

        for (suffix, path, contents) in cleanup_targets {
            context.write_new(&path, contents).unwrap();
            let expected = context.file_state(&path).unwrap().unwrap();
            let exfiltration = attacker.join(format!("exfiltrated-{suffix}.tmp"));
            let (control_source, control_destination) = control_move_paths(&attacker, suffix);
            let (ready, actor_ready) = mpsc::channel();
            let (release, actor_release) = mpsc::sync_channel(0);
            let actor_path = path.clone();
            let actor_exfiltration = exfiltration.clone();
            let actor = spawn_restricted_identity(move || {
                let control = move_path(&control_source, &control_destination, 0);
                actor_ready.send(()).unwrap();
                actor_release
                    .recv_timeout(Duration::from_secs(10))
                    .expect("cleanup race hook did not release the restricted actor");
                let move_out = move_path(&actor_path, &actor_exfiltration, 0);
                let remove = delete_path(&actor_path);
                (control, move_out, remove)
            });

            let callback_path = path.clone();
            let callback_exfiltration = exfiltration.clone();
            let callback_sentinel = sentinel.clone();
            context
                .remove_file_with_pre_delete_hook(&path, &expected, move || {
                    let (control, move_out, remove) = release_race_actor(ready, release, actor);
                    assert_eq!(control, Ok(()), "restricted control rename failed");
                    assert_blocked_by_acl_or_share(move_out, "move a cleanup target outside");
                    assert_blocked_by_acl_or_share(remove, "remove a cleanup target");
                    assert_eq!(fs::read(&callback_path).unwrap(), contents);
                    assert!(!callback_exfiltration.exists());
                    assert_eq!(fs::read(&callback_sentinel).unwrap(), b"outside bytes");
                    Ok(())
                })
                .unwrap();

            assert!(context.file_state(&path).unwrap().is_none());
            assert!(!exfiltration.exists());
            assert_eq!(fs::read(&sentinel).unwrap(), b"outside bytes");
        }
    }

    #[test]
    fn diagnose_documented_rename_forms_on_disposable_files() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let context = MutationContext::open(&target, &git).unwrap();
        let outside_sentinel = work.path().join("outside-sentinel.txt");
        fs::write(&outside_sentinel, b"unchanged outside fixture").unwrap();
        let add_file_parent = open_directory_with_add_file(&target, &context.dacl).unwrap();
        assert_eq!(
            directory_id(&context.target.root),
            directory_id(&add_file_parent),
            "the diagnostic access handle must name the same fixture directory"
        );

        let current_relative = attempt_documented_rename(
            &context,
            "diagnostic-current-source.tmp",
            Path::new("diagnostic-current-destination.tmp"),
            Some(&context.target.root),
            false,
        );
        let relative_with_add_file = attempt_documented_rename(
            &context,
            "diagnostic-add-source.tmp",
            Path::new("diagnostic-add-destination.tmp"),
            Some(&add_file_parent),
            false,
        );
        let relative_extended = attempt_documented_rename(
            &context,
            "diagnostic-extended-source.tmp",
            Path::new("diagnostic-extended-destination.tmp"),
            Some(&add_file_parent),
            true,
        );
        let native_relative = attempt_native_relative_rename(
            &context,
            "diagnostic-native-source.tmp",
            "diagnostic-native-destination.tmp",
            &context.target.root,
        );
        let absolute_legacy = attempt_documented_rename(
            &context,
            "diagnostic-absolute-source.tmp",
            &target.join("diagnostic-absolute-destination.tmp"),
            None,
            false,
        );
        let path_based = attempt_move_file_ex(
            &context,
            "diagnostic-path-source.tmp",
            "diagnostic-path-destination.tmp",
        );
        let matrix = format!(
            "read-only relative={current_relative:?}; relative with FILE_ADD_FILE and FILE_TRAVERSE={relative_with_add_file:?}; FileRenameInfoEx with FILE_ADD_FILE and FILE_TRAVERSE={relative_extended:?}; NtSetInformationFile relative={native_relative:?}; null-root absolute FileRenameInfo={absolute_legacy:?}; MoveFileExW={path_based:?}"
        );
        eprintln!("Windows rename diagnostic matrix: {matrix}");

        assert!(
            current_relative.is_err() && format!("{current_relative:?}").contains("87"),
            "expected the current read-only RootDirectory form to reproduce error 87: {matrix}"
        );
        assert!(
            matches!(relative_with_add_file.as_ref(), Err(error) if error.contains("87")),
            "expected the Win32 relative rename form to reproduce error 87: {matrix}"
        );
        assert!(
            matches!(relative_extended.as_ref(), Err(error) if error.contains("87")),
            "expected the Win32 extended relative rename form to reproduce error 87: {matrix}"
        );
        assert!(
            native_relative.is_ok(),
            "native relative rename through the verified destination handle failed: {matrix}"
        );
        assert!(
            absolute_legacy.is_ok(),
            "absolute legacy rename with null RootDirectory failed: {matrix}"
        );
        assert!(
            path_based.is_ok(),
            "MoveFileExW synthetic baseline failed: {matrix}"
        );
        for path in [
            "diagnostic-native-destination.tmp",
            "diagnostic-absolute-destination.tmp",
            "diagnostic-path-destination.tmp",
        ] {
            assert_eq!(
                fs::read(target.join(path)).unwrap(),
                b"synthetic rename bytes"
            );
        }
        assert_eq!(
            fs::read(outside_sentinel).unwrap(),
            b"unchanged outside fixture"
        );
    }

    #[test]
    fn broad_write_dacl_is_rejected_before_mutation() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        grant_everyone_modify(&target, true);

        let error = match MutationContext::open(&target, &git) {
            Ok(_) => panic!("Everyone write access must be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("DACL"), "unexpected error: {error}");
    }

    #[test]
    fn child_creation_on_selected_root_is_rejected_before_mutation() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let sentinel = target.join("sentinel");
        fs::write(&sentinel, b"unchanged target bytes").unwrap();
        grant_everyone_add_subdirectory(&target);

        let error = match MutationContext::open(&target, &git) {
            Ok(_) => panic!("untrusted creation rights on a protected target must be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("DACL"), "unexpected error: {error}");
        assert_eq!(fs::read(sentinel).unwrap(), b"unchanged target bytes");
        assert!(!target.join(".ai-guidebook").exists());
    }

    #[test]
    fn unsafe_journal_dacl_is_rejected_without_changing_its_bytes() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let context = MutationContext::open(&target, &git).unwrap();
        let journal = git.join("journal.json");
        fs::write(&journal, b"synthetic journal bytes").unwrap();
        grant_everyone_modify(&journal, false);

        let error = match context.file_state(&journal) {
            Ok(_) => panic!("broad journal write access must be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("DACL"), "unexpected error: {error}");
        assert_eq!(fs::read(journal).unwrap(), b"synthetic journal bytes");
    }

    #[test]
    fn new_stage_file_does_not_inherit_untrusted_write_access() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let context = MutationContext::open(&target, &git).unwrap();
        let stage = target.join(".ai-guidebook/stage.tmp");
        let unsafe_parent = stage.parent().unwrap();
        context.create_parent_dirs(&stage).unwrap();
        grant_everyone_inherit_only_modify(unsafe_parent);

        context
            .write_new(&stage, b"protected staged bytes")
            .unwrap_or_else(|error| panic!("safe stage creation failed: {error}"));

        let file = fs::File::open(&stage).unwrap();
        assert_dacl_is_protected(file.as_raw_handle());
        assert_restricted_identity_cannot_open(
            &stage,
            windows_sys::Win32::Storage::FileSystem::FILE_WRITE_DATA,
            0,
        );
        assert_eq!(fs::read(stage).unwrap(), b"protected staged bytes");
    }

    #[test]
    fn new_journal_temporary_does_not_inherit_untrusted_write_access() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let context = MutationContext::open(&target, &git).unwrap();
        let journal = git.join("journal.json");
        grant_everyone_inherit_only_modify(&git);

        context
            .write_atomic(&journal, b"protected journal bytes", true)
            .unwrap_or_else(|error| panic!("safe journal creation failed: {error}"));

        let file = fs::File::open(&journal).unwrap();
        assert_dacl_is_protected(file.as_raw_handle());
        assert_restricted_identity_cannot_open(
            &journal,
            windows_sys::Win32::Storage::FileSystem::FILE_WRITE_DATA,
            0,
        );
        assert_eq!(fs::read(journal).unwrap(), b"protected journal bytes");
    }

    #[test]
    fn new_output_directory_does_not_inherit_untrusted_write_access() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let context = MutationContext::open(&target, &git).unwrap();
        let parent = target.join(".ai-guidebook");
        context
            .create_parent_dirs(&parent.join("placeholder"))
            .unwrap();
        grant_everyone_inherit_only_modify(&parent);
        let nested = parent.join("nested");

        context
            .create_parent_dirs(&nested.join("stage.tmp"))
            .unwrap_or_else(|error| panic!("safe output directory creation failed: {error}"));

        let created = {
            let pinned = context.target.pinned.lock().unwrap();
            std::sync::Arc::clone(
                pinned
                    .get(Path::new(".ai-guidebook/nested"))
                    .expect("created output directory is pinned"),
            )
        };
        assert_dacl_is_protected(created.as_raw_handle());
        assert_restricted_identity_cannot_open(
            &nested,
            windows_sys::Win32::Storage::FileSystem::FILE_ADD_FILE,
            windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS,
        );
    }

    #[test]
    fn directory_reparse_point_is_rejected_without_touching_outside_sentinel() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let outside = work.path().join("outside");
        let redirect = target.join("redirect");
        fs::create_dir(&outside).unwrap();
        let sentinel = outside.join("sentinel");
        fs::write(&sentinel, b"outside bytes").unwrap();
        std::os::windows::fs::symlink_dir(&outside, &redirect)
            .expect("Windows CI must permit fixture symlink creation");
        let context = MutationContext::open(&target, &git).unwrap();

        let error = match context.write_new(&redirect.join("should-not-exist"), b"outside write") {
            Ok(()) => panic!("directory reparse points must be rejected"),
            Err(error) => error,
        };

        assert!(
            error.contains("symlink")
                || error.contains("reparse point")
                || error.contains("without following"),
            "unexpected reparse-point rejection: {error}"
        );
        assert!(!outside.join("should-not-exist").exists());
        assert_eq!(fs::read(sentinel).unwrap(), b"outside bytes");
    }
}
