# Implementation decisions

## 10-15-2026 — One generated model document

Small source modules keep edits understandable; `GUIDEBOOK.md` supplies the complete policy and public profile in one retrieval. Stable IDs allow precise review. The generator checks every generated entry point byte-for-byte so adapters cannot drift silently. Generated output includes a source digest, version, and source list.

## 10-15-2026 — Standing policy with honest boundaries

The guidebook is mandatory user guidance wherever loaded. It cannot override system/platform rules, install itself across every product, create durable memory by prose, or guarantee model compliance. Models must verify that the content was read. Local snapshots make offline use possible without claiming freshness.

## 10-15-2026 — Python maintenance exception (superseded)

The initial implementation used Python document tooling. The user subsequently explicitly required Rust for everything. That decision supersedes the maintenance exception: all newly authored executable tooling and tests are Rust. The prior implementation and measured results remain in Git history and dated audit records; they are not current instructions.

## 10-15-2026 — Copyleft and existing projects

The repository already contained an AGPL license selected at creation. That complete license is preserved, consistent with the preference that covered improvements remain open. Network-service AGPL choices and third-party licensing need their own analysis. Installing instructions must not relicense another project or change its existing notices.

## 10-15-2026 — Preserving existing instructions

The installer defaults to preview, requires an explicit `--apply` to write, updates only a marked section, and refuses malformed markers, symlinks, path escapes, unmanaged snapshot replacement, and targets inside this guidebook. It never commits or pushes. Existing model adapters are preserved and reported for integration rather than replaced. Fixed local targets and digest locks support audit; they are not cryptographic authentication of a remote policy publisher.

## 10-15-2026 — Integrated audits and Rust replacement

Preserve the existing modular policy and 125 stable IDs. A Rust builder combines those modules with the 45-rule verification register and self-check/evolution procedures. The combined GUIDEBOOK.md remains the complete model entry point. The typed register supplies explicit scope, review deadlines, and machine-checkable evidence dispositions; it complements rather than discards the earlier policy.

Rust is mandatory for implementation and tests. Markdown, JSON, TOML, workflow YAML, and short workflow invocations remain declarative support, not a second programming implementation. The adopter keeps preview-by-default, marked sections, digest checks, and adapter preservation. File writes are individually atomic, not a multi-file transaction; interruption and hostile concurrent filesystem mutation remain explicit limits.
