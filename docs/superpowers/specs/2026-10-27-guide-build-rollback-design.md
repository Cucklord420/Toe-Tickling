# Guidebook build rollback design

## Intent

Keep the generated guide, lock, and adapters at one coherent revision when the builder reports an output write failure. `build` already validates and renders all output bytes before beginning file replacement, but its sequential persists can still leave earlier files changed if a later persist fails.

## Goals and limits

- Preserve current output paths, deterministic bytes, per-file atomic replacement, output-count return value, and read-only `check` behavior.
- Stage every new output and a recoverable copy of every existing destination before the first replacement.
- On a reported commit failure, restore replaced outputs in reverse order. If restoration itself fails, retain the needed backups and report exact recovery paths rather than silently discarding them.
- Revalidate every output path's ancestors immediately before replacement and rollback; do not follow a symlink introduced after staging.
- Preserve destination permissions after tempfile finalization on Windows, where `keep` and `persist` normalize file attributes.
- Keep all new executable code and tests in Rust; add no dependencies.
- Cover build-validation failure, output collision, an actual later persist failure, an ancestor-symlink swap, and Windows readonly recovery with disposable fixtures.

An abrupt process termination or host loss between independent filesystem renames is not a multi-file atomic transaction. This change covers returned output errors and makes rollback failures recoverable; it will not claim power-loss atomicity.

## Approaches

1. **Validation-only preflight.** This prevents errors discovered while rendering or resolving output paths from changing files, but it cannot recover after a later persist fails. The existing builder already performs this preflight, so it does not satisfy the remaining finding.
2. **Publish a versioned generation directory and switch a pointer.** This gives one publication point, but changes the repository's established direct output paths and consumer behavior.
3. **Stage new files and backups, then commit with reverse rollback.** This preserves current paths and per-file replacement. On failure, it restores prior bytes and permissions; if rollback is blocked, it retains the backups and reports how to recover. This is the chosen approach.

## Implementation choices

- Keep `build` responsible for policy validation, rendering, and destination preflight.
- Separate staging and commit in `src/guide.rs` so failure injection can exercise the commit path without adding a production environment variable or external test helper.
- Store each backup beside its destination so restoration uses the same filesystem. Preserve existing file permissions in both backup and restored file.
- Re-resolve each relative destination through the root's symlink-safe path validator before `persist`, before `remove_file`, and before rollback replacement. If validation fails, stop the commit and retain the backup rather than writing through the changed path.
- After retaining a backup or persisting a restoration, reapply the original permissions through the returned file handle; retain and report the backup even if permission restoration fails.
- Before restoring, verify the destination still contains the bytes installed by this transaction (or is unchanged from its original state). If it contains a third-party change, do not overwrite it; retain the backup and report the conflict.
- Return the original commit error plus rollback results. A complete rollback leaves no backup/stage files; an incomplete rollback leaves precise recovery paths.

## Review calls

- Treat in-process write/persist failures as the required transaction boundary. Do not claim crash/power-loss atomicity across independent files.
- Treat filesystem path validation and Windows readonly metadata as part of the transaction boundary; preserve the exact recovery path and report metadata-restoration failures.
- Do not widen this work into adopter transactions or policy changes.
