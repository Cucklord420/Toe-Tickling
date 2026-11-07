// SPDX-License-Identifier: AGPL-3.0-only
//! Protected, handle-anchored filesystem access for adoption mutations.

use super::{JournalFileState, JournalPermissions};
#[cfg(unix)]
use cap_fs_ext::OsMetadataExt;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
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
        let child = parent.open_dir_nofollow(&name).map_err(|error| {
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
        let mut options = cap_std::fs::OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(windows)]
        {
            use cap_std::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
                FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_DATA, READ_CONTROL, SYNCHRONIZE,
            };
            options
                .access_mode(
                    DELETE
                        | FILE_READ_DATA
                        | FILE_WRITE_DATA
                        | FILE_READ_ATTRIBUTES
                        | READ_CONTROL
                        | SYNCHRONIZE,
                )
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let mut file = parent
            .directory
            .open_with(&parent.leaf, &options)
            .map_err(|error| format!("Cannot create {}: {error}", path.display()))?;
        let verification = file
            .metadata()
            .map_err(|error| format!("Cannot inspect new file {}: {error}", path.display()))
            .and_then(|metadata| {
                verify_file(
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
                    rename_file_relative(
                        &parent.directory,
                        &temp,
                        &parent.directory,
                        &parent.leaf,
                        &source_state,
                        destination_state.as_ref(),
                        &self.dacl,
                    )
                    .map_err(|error| {
                        format!("Cannot atomically publish {}: {error}", path.display())
                    })
                }
            }
            #[cfg(not(windows))]
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
            delete_open_handle(file.as_raw_handle())
        }
        #[cfg(not(windows))]
        {
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
            return rename_file_relative(
                &from_parent.directory,
                &from_parent.leaf,
                &to_parent.directory,
                &to_parent.leaf,
                expected_source,
                expected_destination,
                &self.dacl,
            );
        }
        #[cfg(not(windows))]
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
fn create_directory_relative(parent: &Dir, name: &OsStr) -> std::io::Result<Dir> {
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
                FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, SYNCHRONIZE,
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
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE | DELETE,
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
fn rename_file_relative(
    source_parent: &Dir,
    source_name: &OsStr,
    destination_parent: &Dir,
    destination_name: &OsStr,
    expected_source: &JournalFileState,
    expected_destination: Option<&JournalFileState>,
    policy: &windows_acl::DaclPolicy,
) -> super::Result<()> {
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
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FILE_RENAME_INFO, FILE_RENAME_INFO_0, FileRenameInfo, SetFileInformationByHandle,
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
    let buffer_length = std::mem::offset_of!(FILE_RENAME_INFO, FileName)
        .checked_add(filename_bytes)
        .ok_or("Rename information length overflowed")?;
    let buffer_length = buffer_length.max(std::mem::size_of::<FILE_RENAME_INFO>());
    let buffer_length_u32 = u32::try_from(buffer_length)
        .map_err(|_| "Rename information exceeds the Windows buffer limit")?;
    let words = buffer_length.div_ceil(std::mem::size_of::<usize>());
    let mut buffer = vec![0usize; words];
    let information = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*information).Anonymous = FILE_RENAME_INFO_0 {
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
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileRenameInfo,
            information.cast::<std::ffi::c_void>(),
            buffer_length_u32,
        )
    } == 0
    {
        return Err(format!(
            "Cannot rename through the verified destination handle: {}",
            std::io::Error::last_os_error()
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
                let child = parent.open_dir_nofollow(name).map_err(|error| {
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
            let child = current.open_dir_nofollow(name).map_err(|error| {
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
                    let child = current.open_dir_nofollow(name).map_err(|error| {
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
                        let child = current.open_dir_nofollow(name).map_err(|open_error| {
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
                    windows_acl::DaclScope::ProtectedObject,
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
    #[cfg(windows)] _policy: &windows_acl::DaclPolicy,
) -> std::io::Result<Dir> {
    #[cfg(windows)]
    {
        create_directory_relative(parent, name)
    }
    #[cfg(unix)]
    {
        let mut builder = cap_std::fs::DirBuilder::new();
        use cap_std::fs::DirBuilderExt;
        builder.mode(0o700);
        parent.create_dir_with(name, &builder)?;
        parent.open_dir_nofollow(name)
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
        let mut options = cap_std::fs::OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(windows)]
        {
            use cap_std::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
                FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_DATA, READ_CONTROL, SYNCHRONIZE,
            };
            options
                .access_mode(
                    DELETE
                        | FILE_READ_DATA
                        | FILE_WRITE_DATA
                        | FILE_READ_ATTRIBUTES
                        | READ_CONTROL
                        | SYNCHRONIZE,
                )
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        match parent.directory.open_with(&name, &options) {
            Ok(mut file) => {
                let verification =
                    file.metadata()
                        .map_err(|error| error.to_string())
                        .and_then(|metadata| {
                            verify_file(
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
        Foundation::{CloseHandle, HANDLE, LocalFree},
        Security::Authorization::{ConvertStringSidToSidW, GetSecurityInfo, SE_FILE_OBJECT},
        Security::{
            ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, CreateWellKnownSid,
            DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation, GetLengthSid,
            GetSecurityDescriptorDacl, GetTokenInformation, IsValidSid, OWNER_SECURITY_INFORMATION,
            PSID, TOKEN_QUERY, TOKEN_USER, TokenUser, WELL_KNOWN_SID_TYPE,
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
    const GENERIC_RIGHTS: u32 = GENERIC_READ | GENERIC_WRITE | GENERIC_EXECUTE | GENERIC_ALL;
    const KNOWN_FILE_ACCESS_RIGHTS: u32 =
        0x000f_01ff | 0x0010_0000 | ACCESS_SYSTEM_SECURITY | MAXIMUM_ALLOWED | GENERIC_RIGHTS;
    const CHILD_CREATION_RIGHTS: u32 = FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum DaclScope {
        ExistingPathAncestry,
        ProtectedObject,
    }

    pub(super) struct DaclPolicy {
        trusted_sids: Vec<Vec<u8>>,
    }

    impl DaclPolicy {
        pub(super) fn current() -> super::super::super::Result<Self> {
            let current = current_user_sid()?;
            let system = well_known_sid(WinLocalSystemSid)?;
            let admins = well_known_sid(WinBuiltinAdministratorsSid)?;
            let creator_owner = well_known_sid(WinCreatorOwnerSid)?;
            let trusted_installer =
                string_sid("S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464")?;
            Ok(Self {
                trusted_sids: vec![current, system, admins, creator_owner, trusted_installer],
            })
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
                    return Err(
                        "Mutation object's DACL contains generic, maximum-allowed, or unknown access rights"
                            .into(),
                    );
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
                    if !self.trusted(sid) && !untrusted_mutation_grant_allowed(mask, scope) {
                        return Err(format!(
                            "Mutation object's DACL grants another local user mutation rights (mask 0x{:08x})",
                            mask & mutation_rights()
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
    }

    fn access_mask_is_supported(mask: u32) -> bool {
        mask & (GENERIC_RIGHTS | MAXIMUM_ALLOWED) == 0 && mask & !KNOWN_FILE_ACCESS_RIGHTS == 0
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
        use super::DELETE;
        use super::{
            DaclScope, GENERIC_ALL, GENERIC_EXECUTE, GENERIC_READ, GENERIC_WRITE, MAXIMUM_ALLOWED,
            access_mask_is_supported, mutation_rights, untrusted_mutation_grant_allowed,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_DELETE_CHILD, FILE_WRITE_DATA,
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
            for creation_right in [FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY] {
                assert!(untrusted_mutation_grant_allowed(
                    creation_right,
                    DaclScope::ExistingPathAncestry
                ));
                assert!(!untrusted_mutation_grant_allowed(
                    creation_right,
                    DaclScope::ProtectedObject
                ));
                for forbidden_right in [FILE_DELETE_CHILD, DELETE, FILE_WRITE_DATA] {
                    assert!(!untrusted_mutation_grant_allowed(
                        creation_right | forbidden_right,
                        DaclScope::ExistingPathAncestry
                    ));
                }
            }
        }

        #[test]
        fn generic_maximum_and_unknown_access_masks_are_rejected() {
            for unsupported in [
                GENERIC_READ,
                GENERIC_WRITE,
                GENERIC_EXECUTE,
                GENERIC_ALL,
                MAXIMUM_ALLOWED,
                0x0080_0000,
            ] {
                assert!(!access_mask_is_supported(unsupported));
            }
            assert!(access_mask_is_supported(0x0000_0001));
            assert!(access_mask_is_supported(FILE_ADD_SUBDIRECTORY));
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
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

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
    use super::MutationContext;
    use std::{fs, path::Path, process::Command};

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
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let pinned = target.join("pinned");
        let renamed = target.join("renamed");
        fs::create_dir(&pinned).unwrap();
        let context = MutationContext::open(&target, &git).unwrap();
        context.pin_directory(&pinned).unwrap();

        let error = fs::rename(&pinned, &renamed).unwrap_err();

        assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Other
            ),
            "unexpected rename error: {error}"
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
    fn unsafe_inherited_stage_dacl_is_rejected_before_content_is_written() {
        let work = mutation_fixture_tempdir();
        let (target, git) = target_and_git(work.path());
        let context = MutationContext::open(&target, &git).unwrap();
        let stage = target.join(".ai-guidebook/stage.tmp");
        let unsafe_parent = stage.parent().unwrap();
        let outside = work.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let sentinel = outside.join("sentinel");
        fs::write(&sentinel, b"outside bytes").unwrap();
        context.create_parent_dirs(&stage).unwrap();
        grant_everyone_modify(unsafe_parent, true);

        let error = match context.write_new(&stage, b"must not be written") {
            Ok(()) => panic!("unsafe inherited stage DACL must be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("DACL"), "unexpected error: {error}");
        assert!(!stage.exists());
        let journal = unsafe_parent.join("journal.json");
        let error = match context.write_atomic(&journal, b"must not be written", true) {
            Ok(()) => panic!("unsafe inherited journal DACL must be rejected"),
            Err(error) => error,
        };
        assert!(error.contains("DACL"), "unexpected error: {error}");
        assert!(!journal.exists());
        assert!(fs::read_dir(unsafe_parent).unwrap().next().is_none());
        assert_eq!(fs::read(sentinel).unwrap(), b"outside bytes");
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

        assert!(error.contains("symlink") || error.contains("without following"));
        assert!(!outside.join("should-not-exist").exists());
        assert_eq!(fs::read(sentinel).unwrap(), b"outside bytes");
    }
}
