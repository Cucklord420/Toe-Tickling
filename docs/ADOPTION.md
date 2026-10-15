# Adoption across models and projects

## Any chat or model

Point the model at the canonical `GUIDEBOOK.md`, or attach/paste the full file when it cannot browse. `prompts/USE_WITH_ANY_MODEL.md` gives a reusable instruction. A model must actually read the file before claiming compliance. The public profile supplies no private account, health, financial, or precise-address context.

`prompts/STANDING_MEMORY.md` is a short memory/custom-instruction entry to save using a platform's real supported mechanism. This file is not itself a remote memory write. Do not claim a global setting was changed merely because it exists here.

## A Git project

Run the guidebook maintenance CLI from a checkout of this repository:

```sh
python3 scripts/guidebook.py adopt /absolute/path/to/project
python3 scripts/guidebook.py adopt /absolute/path/to/project --apply
```

The first command previews. The second writes the additions. The target must be the root of an existing Git working tree. Paths containing spaces work when quoted according to the local shell. The command works from another current directory when the script itself is addressed correctly.

Writes are preflighted, and each file replacement is atomic. The complete multi-file installation is not a filesystem transaction: an I/O failure or interruption can leave some additions written. Review the resulting diff and snapshot/lock before retrying; the tool rejects a mismatched existing snapshot instead of silently replacing it. It does not claim crash-proof installation or protection from hostile concurrent filesystem changes.

The installer creates `.ai-guidebook/GUIDEBOOK.md`, a `policy-lock.json` with version/source URL/digests, and a marked `AGENTS.md` section. It appends a marked private-file section to `.gitignore`. Existing instructions outside those blocks remain unchanged. Reapplying is idempotent. A guidebook update is explicit: update this checkout, run checks, preview adoption again, inspect the diff, and commit under the project's authorization.

Adapters created only when missing: `CLAUDE.md`, `GEMINI.md`, `.github/copilot-instructions.md`, and `.cursor/rules/justin-guidebook.mdc`. If one exists, it is preserved and listed. Integrate the local guidebook reference into it only after reading its existing instructions; never overwrite unrelated behavior. `AGENTS.md` readers have the universal entry point regardless of adapter creation.

## Platform behavior

| Environment | Included entry point | Required behavior |
| --- | --- | --- |
| AGENTS.md readers / OpenCode | Root `AGENTS.md` | Explicitly read the local full guide; a Markdown link is not an import |
| Claude Code | `CLAUDE.md` with explicit import | Import full guide plus preserve project instructions |
| GitHub Copilot | `.github/copilot-instructions.md` | Load the full guide through the repository context/tools |
| Gemini CLI | `GEMINI.md` with local import | Load the full guide and applicable project context |
| Cursor | Always-applied `.mdc` rule and `AGENTS.md` | Follow the bootstrap and explicitly read the guide |
| Other models | Full `GUIDEBOOK.md` attachment or retrieval | Do not assume URL memory or automatic remote execution |

Compatibility criteria: documented instruction entry points with an explicit path to one complete guide; actual support depends on the installed product/version and must be verified in that product.

These files are contextual instructions, not universal account settings. They do not by themselves install organization-wide policies, enforce branch protection, create automations, or alter already-open sessions. No provider-global settings or private records are changed by the installer.

## Local private context

Use the blank template only if desired, with Justin choosing what to supply. Keep completed copies outside the repository or in an ignored path. Never commit a filled template; never assume gitignore is encryption. Project-specific private data remains private even when this guidebook is public.

## Verify adoption

Inspect `git diff`, confirm the snapshot digest against its lock, and confirm the target model loaded the guide in its context. A successful file installation proves files exist and checksums match; it does not prove a model will obey every rule. The installer does not modify remote repositories until an authorized caller reviews and publishes the resulting diff.
