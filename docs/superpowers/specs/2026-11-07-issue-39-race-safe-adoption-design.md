# Issue #39: Race-safe adoption and recovery

**Date:** 11-07-2026 (America/New_York)

**Repository:** `Cucklord420/Toe-Tickling`

**Base reviewed:** `db2146f781e9c088d3694cfb77bd5434ec90839a` (main)

**Canonical tracker:** [Issue #39](https://github.com/Cucklord420/Toe-Tickling/issues/39)
**Execution:** existing issue-specific work only; no new Issue. Create one canonical Draft PR after the implementation and exact-head validation are ready.

## Intent

Make both installation (`--apply`) and rollback/cleanup (`--recover`) safe against a different local user racing path replacement. Keep read-only preview unchanged. The owner has authorized the complete Rust fix and directed that `--apply` stay paused until its race regressions pass. Treat `--recover` with the same fail-closed gate because it can restore files, change permissions, remove stages, and delete the journal.

## Current behavior and root cause

- On current `main`, public `apply` returns a pause error, but `recover` remains active.
- The private transaction engine and recovery helpers perform reads and mutations through separately checked pathnames. These include destination staging and rename, journal create/update/remove, permission changes, rollback, and stage cleanup.
- A no-follow check followed by a later pathname operation leaves a replacement window. An open Unix directory handle is stable when renamed; it does not prevent the directory itself from being moved outside the selected repository. On Windows, the directory handle can prevent rename only if it is opened without `FILE_SHARE_DELETE`.
- The already-dirty `codex/issue-39-race-safe-adoption` checkout is stale and contains an incomplete handle-based attempt. It remains untouched and is not an implementation base.

## Threat boundary

- Protect against path swaps by other local users. Same-effective-UID processes and root/administrator are outside the claim because they already have authority to change the selected files and directory controls.
- On POSIX, mutations are allowed only when every target, output-parent, and Git-metadata directory is opened without following links and its ownership/permission chain prevents other UIDs from renaming it. A writable sticky ancestor is accepted only when the next child is owned by the current effective UID. Running as root fails closed. On macOS, an ACL or volume state that cannot be classified conclusively also fails closed.
- On Windows, mutations are allowed only on a verified local volume. Each path component is opened relative to the already verified parent handle, with reparse-point following disabled; opening a later component by its full absolute path is not sufficient. Every directory handle is held without `FILE_SHARE_DELETE` for the transaction. The DACL from each opened handle is inspected and evaluated for other-user rights that can write, add, delete, replace, or change the ACL/owner of target outputs, stages, journals, or ancestry. Any null, unreadable, unsupported, or ambiguous descriptor fails closed. UNC and remote volumes fail closed. Administrator-level changes are outside the claim.
- Unsupported filesystems, ambiguous metadata, or failed identity/permission checks leave files, stages, and journals untouched. Do not describe these limits as verified race prevention.

## Goals

- Route all installation and recovery mutations through a Rust capability context holding no-follow handles for the target path and the associated Git metadata directory.
- Make directory creation, journal reads/writes/removal, staging, promotion, permission updates, rollback, and cleanup relative to the verified handles.
- Revalidate path identity and exact journal/file state before each consequential operation; fail closed on a mismatch.
- Preserve current journal versions and recovery semantics, including transition markers, user-edit preservation, partial install rollback, and retry after incomplete recovery.
- Keep `--apply` and `--recover` unavailable until the new regression suite passes on supported CI platforms; preview stays available and read-only.
- Use Rust-only tests with disposable repositories, synthetic bytes, and an outside sentinel. Add platform CI for the race-specific paths.

## Non-goals

- Do not claim protection against a process running as the same effective UID, root/administrator, kernel/filesystem compromise, or a hostile filesystem that violates documented handle and permission behavior.
- Do not automatically remove unknown stages or journals, change journal format, change preview output, change unrelated CLI defaults, or create a second canonical tracker.
- Do not change source checkouts, account settings, Git history, or real user repositories during tests.

## Approaches

1. **Capability handles plus protected-path checks — selected working approach.** Use handle-relative no-follow operations everywhere. POSIX additionally rejects ancestry that another UID could replace; Windows blocks directory rename with no-delete-sharing handles on local volumes. macOS and other environments fail closed if ACL/volume safety cannot be proven. This restores mutation only where the threat boundary is enforced and testable.
2. **Windows-only mutation, Unix permanently paused.** This has a narrower implementation but unnecessarily removes Linux support even where POSIX permissions prevent other users from changing ancestry. Keep as fallback if the POSIX guard cannot be proven.
3. **Repeated pathname checks with capability handles but no protection against directory movement.** Rejected because a Unix directory can be moved after the final check and receive writes through its old handle.

## Design

Add one internal mutation context that owns verified target and Git-metadata directory chains. Each chain starts at the filesystem root, opens each component without following symlinks/reparse points, records stable identity, and holds all handles through apply or recovery. Output paths remain validated relative components; no absolute path is used for a mutation after the chains are opened.

For POSIX, verify the effective UID is non-root and inspect ownership, mode, sticky-bit rules, and macOS ACL state through the opened objects. A group/world-writable non-sticky ancestor rejects the operation. A sticky writable ancestor is accepted only when its next child is owned by the effective UID. A named ACL grant or ACL state that cannot be conclusively classified rejects mutation. The same checks apply to the linked-worktree Git metadata directory because it may be outside the target checkout.

For Windows, open the root and then one child component at a time relative to the preceding verified directory handle, with reparse-point processing disabled for that component. Use the documented `RootDirectory`/single-component form; do not fall back to a full multi-component pathname. Open each child with `FILE_OPEN_REPARSE_POINT`, inspect the returned handle for directory/type and reparse status, and reject reparse points. Do not combine `FILE_DIRECTORY_FILE` with `FILE_OPEN_REPARSE_POINT`, because Microsoft documents a closed compatibility list for `FILE_DIRECTORY_FILE` that omits this combination; if the needed open cannot be made and verified, fail closed. Open directories with a share mode that permits read/write sharing but omits `FILE_SHARE_DELETE`; keep every parent handle alive until the transaction ends. This blocks ancestry renames while open, while the conservative parent DACL checks prevent other users from racing child names. Require a local volume. Read each object's DACL from the opened handle and apply a conservative trustee policy; checking only the current process token cannot prove what other local users can do. Mutation-capable allow ACEs are accepted only for the current user SID, SYSTEM, or built-in Administrators. Reject a null or unreadable DACL, unrecognized trustees or ACE types with mutation rights, unresolved generic rights, owner ambiguity, and any inherited or explicit broad grant that could let another user write, add, delete, replace, change ACLs, or change owners. Apply this policy to each target and Git-metadata ancestor, existing destination and journal file, and every created output directory, stage, rollback stage, and temporary journal file. Create new objects with restrictive non-inheriting security attributes where available, then verify the DACL through the opened handle before writing contents or relying on them. Windows access-mask tests must cover data writes, append, directory child creation/deletion, object deletion, ACL changes, owner changes, and generic write/all mappings. If any descriptor or right cannot be conclusively classified, leave both installation and recovery paused on that path.

Convert every write path, not only final destination promotion: journal creation/update/removal; target parent creation; stage and rollback-stage creation; exact-byte readback; atomic replacement; Windows read-only transitions; recovery cleanup; and error rollback. Read-only Git identity checks may still use Git, but the opened target and `.git` identities must be revalidated before and after those checks. Preserve journal v1/v2 migration to v3 and the existing checksum/transition protocol.

The public `apply` and `recover` entry points share the protected mutation context. The CLI rejects disabled/unsupported mutation before planning, target inspection, journal inspection, or success JSON. Keep `apply` paused in the implementation branch until the concurrent swap tests, full recovery regressions, formatting, strict Clippy, and exact-head Ubuntu/Windows CI pass. Add a macOS job for ACL classification; if it cannot prove the no-ACL or safe-ACL case, that platform remains fail-closed and the Issue stays open.

## Required regressions

- POSIX: unprotected writable ancestry and root execution reject apply and recovery before touching a sentinel, output, stage, or journal; a protected disposable target can complete and recover.
- Windows: link/reparse-point ancestors are rejected by handle-relative component opens; local-volume and DACL checks reject paths another user can mutate; held no-delete handles prevent ancestry rename; broad and inherited write grants are rejected; newly created directories, stages, rollback stages, and journal temporaries have restrictive verified DACLs before content writes; concurrent child replacement is blocked or detected before mutation; verify the outside sentinel and outside directory remain unchanged.
- All tested platforms: swaps before final verification fail closed; conflicting user edits remain intact; journal versions and permission transition states retain their current behavior; incomplete recovery can be retried; no stage/journal is removed when identity validation fails.
- CLI: unsupported apply/recover fail before inspecting invalid targets/journals and never emit `applied` or `recovered` JSON; preview remains read-only.

## Rollback and completion

Keep both mutations paused until the race and failure regressions pass. If a supported platform fails the safety checks, keep that platform disabled and leave Issue #39 open. Do not merge or close the Issue until independent review, exact-head CI, supported-platform behavior, and default-branch verification are complete. Rollback is a reviewed revert; preserve existing journal and target data.

## Technical references reviewed

- `cap-std` 4.0.3 `Dir` documentation: directory-relative filesystem operations; Windows callers must supply handles opened without `FILE_SHARE_DELETE` to prevent races.
- Microsoft `CreateFile` sharing documentation: omitting `FILE_SHARE_DELETE` prevents concurrent delete/rename access while the handle remains open.
- Apple filesystem security documentation: ACLs supplement traditional mode bits, so macOS mutation must fail closed if the effective ACL boundary cannot be verified.
- Microsoft `GetSecurityInfo` and file-access-right documentation: inspect security descriptors on opened handles and account for child-creation/deletion, write, delete, ACL, and owner rights.
- Microsoft `NtCreateFile` documentation: a directory handle can anchor a single-component relative open, the supplied security descriptor applies at creation, and `FILE_OPEN_REPARSE_POINT` bypasses normal reparse processing. Avoid undocumented flag combinations and fail closed when the documented form cannot be verified.
- Current project rules: guidebook v1.2.5, read from `main` commit `db2146f781e9c088d3694cfb77bd5434ec90839a`, blob `527c3da11c93dfc0e9fba9ac8caa999e805e671f`.
