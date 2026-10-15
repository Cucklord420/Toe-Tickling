# Implementation decisions

## 10-15-2026 — One generated model document

Small source modules keep edits understandable; `GUIDEBOOK.md` supplies the complete policy and public profile in one retrieval. Stable IDs allow precise review. The generator checks every generated entry point byte-for-byte so adapters cannot drift silently. Generated output includes a source digest, version, and source list.

## 10-15-2026 — Standing policy with honest boundaries

The guidebook is mandatory user guidance wherever loaded. It cannot override system/platform rules, install itself across every product, create durable memory by prose, or guarantee model compliance. Models must verify that the content was read. Local snapshots make offline use possible without claiming freshness.

## 10-15-2026 — Python maintenance exception

The repository contains documents rather than a user application. Its maintenance/installation glue uses only Python 3.10+ standard-library modules and Git, avoiding a compiler, third-party packages, paid infrastructure, or an extra installation for basic document checks. This is a narrow and explicit exception to Rust-first application development, not a reversal of that preference. New application logic remains Rust-first.

## 10-15-2026 — Copyleft and existing projects

The repository already contained an AGPL license selected at creation. That complete license is preserved, consistent with the preference that covered improvements remain open. Network-service AGPL choices and third-party licensing need their own analysis. Installing instructions must not relicense another project or change its existing notices.

## 10-15-2026 — Preserving existing instructions

The installer defaults to preview, requires an explicit `--apply` to write, updates only a marked section, and refuses malformed markers, symlinks, path escapes, unmanaged snapshot replacement, and targets inside this guidebook. It never commits or pushes. Existing model adapters are preserved and reported for integration rather than replaced. Fixed local targets and digest locks support audit; they are not cryptographic authentication of a remote policy publisher.
