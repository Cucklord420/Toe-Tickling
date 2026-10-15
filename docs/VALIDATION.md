# Validation scope

Measured on 10-15-2026 using Python 3.12.14, Git, and Linux x86_64. This is not a claim that prose can enforce model behavior.

- `python3 scripts/guidebook.py build`: generated nine deterministic model/bundle artifacts.
- `python3 scripts/guidebook.py check`: passed; 125 stable rule IDs, 39 public repository files, matching source/bundle digests, valid local links, selected private-data patterns, and immutable/read-only CI configuration. The complete model guide contains 7,308 words.
- `python3 -m unittest discover -s tests -v`: 45 tests passed with no skipped tests in the final local run (6.677 seconds). CLI regressions invoke real child Python/Git processes in temporary directories, including unrelated working directories and paths with spaces. One additional unit case injects an I/O replacement failure to check preservation and temporary-file cleanup; it is not represented as a physical storage failure test.
- Python's AST parser accepted both Python files with the Python 3.10 grammar. This is syntax verification, not a claim of native execution on Python 3.10.
- The existing repository's AGPL license was preserved byte-for-byte: Git blob `0ad25db4bd1d86c452db3f9602ccdbe172438f52` and SHA-256 `8486a10c4393cee1c25392769ddd3b2d6c242d6ec7928e1414efff7dfb2f07ef`.

Coverage includes real CLI startup/build/check, deterministic output, generated-file drift, source changes/missing sources, dates/versions/unique IDs, required instruction-audit sources/rules/adapters, path traversal, symlinks and ancestors, broken/escaping local links, selected credential/contact patterns, ignored local context versus forced tracked private files, mutable action pins, token permissions, preview without mutation, full snapshot/digest installation, preserving existing instructions/gitignore/adapters, repeat-install idempotency, malformed/reversed managed markers, tampered/unmanaged snapshots, invalid/non-root targets, non-directory parents, safe updates, injected replacement failure cleanup, and confirming the installer creates no commit.

Validated guidebook SHA-256: `7e7ea0e5fd53fb6d8e89bf8c7a72b915dc3c555d2af9c949412d75b2aadcf887`.

Validated maintenance script SHA-256: `e80694f6d6a0a3686c79eed8b10da0892394f5f0d65f04f423cf815b4dc28552`.

Validated regression test source SHA-256: `76a318489752c80f28dc7980e25a6ed27b8e6ea221ac1ffb624a048974dfb6b4`.

Limits: no provider-specific live model compliance tests, no account-global settings or cross-session memory writes, and no native Windows/macOS execution in this local environment. Local instruction tests use disposable Git projects and synthetic text; they do not touch client data or production repositories. Pattern checks are incomplete by nature and were supplemented by a review of the public contents.

Remote CI is configured in `.github/workflows/validate.yml`; its actual status must be read from GitHub Actions for the published commit. Local passes do not establish a remote pass. Publication and durable platform memory are separate from these local tests.
