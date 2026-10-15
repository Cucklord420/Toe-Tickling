# Provenance and review coverage

Review date: 10-15-2026 UTC. This is an initial scoped policy review, not a claim that every historical message, repository line, release asset, or live system was audited.

## What was visible

The authenticated repository inventory returned 13 repositories: seven public and six private, including one archived public repository. Default-branch recursive Git tree inventories were retrieved for all 13 without tree truncation. Root README files were fetched for all 13. Candidate instruction, audit, preference, and project-context paths were identified separately. Full local tree inventories and fetched private sources remain outside this public checkout.

At the initial target snapshot, Toe-Tickling `main` was `7e6403060951c7a646466adbdff7a8bd5a4fb45f`, containing README.md and LICENSE. The branch API returned only main and the pull-request list was empty. This establishes no visibility into another agent's unpublished work. Before integration, recheck published branches and pull requests; do not imply continuous observation.

## Public rule provenance

The public operating guidance in [MRI-Drive-Maker AGENTS.md at the inspected revision](https://github.com/Cucklord420/MRI-Drive-Maker/blob/786de8d73eb469936540974914f83309fa80184d/AGENTS.md) supplies the existing evidence, reversible-change, Rust preference, safety, recovery, privacy, cost, and delivery principles. Equivalent instruction text was found in several private repositories. Mirror and theme project documentation was treated as project evidence, not automatically as user-authored global policy.

## Private source review

Selected private agent instructions, current project documentation, licensing policy, audit reports, all eleven archived project-settings instruction records, an operational goal prompt, and selected historical user turns were inspected for reusable intent. Some downloaded documents received targeted searches or excerpts rather than full semantic review. Fetching a file is not the same as reading it.

The archive's project settings contained explicit instructions for three projects and no custom instructions for eight at that historical capture. Those snapshots do not establish present live settings. Selected historical user turns supported scoped written vehicle communication and demonstrated how explicit corrections must supersede assistant elaborations. Private hardware inventories, financial data, personal ergonomics, exact purchasing filters, account links, chat IDs, and raw text are not included here.

Financial and EV rules are scoped profiles, not a universal instruction to give every task a financial or vehicle focus. Operational goal prompts supplied synthesis about checkpointing, idempotency, provenance, and completion; their one-task operational permissions were not imported as global permissions.

## Register source classes

| Source value | Meaning |
| --- | --- |
| `user-current` | The current request for a living, audited, self-checking guide and tested adoption. |
| `public-existing` | Existing public operating rules or directly matching established requirements. |
| `private-reviewed` | Sanitized scoped requirement derived from reviewed private guidance; raw source remains private. |
| `synthesis` | A proposed practical mechanism combining reviewed intent and engineering judgment; not a verbatim historical user rule. |

The user's later explicit requirement makes all newly authored executable code and tests Rust across projects; the earlier suitability preference is superseded. It does not require Markdown or JSON to be disguised as source code. The operational documentation records this current override.

## Known limits and follow-up intake

- Complete historical-message review, binary attachments, private release archives, live ChatGPT settings, all repository branches/history/issues, and other agents' local files were not inspected exhaustively.
- Retrieved chat exports mix user turns and assistant text. A prior assistant saying “saved” is not proof of durable memory or user endorsement.
- No live model was configured, no application deployment was performed, and no production host, scheduler, financial account, or vehicle vendor was changed by this work.
- No behavioral scenario is marked passed merely because it is written down. The Rust regression suite validates tooling; adoption scenarios require the actual target model/runtime.

For future review, enumerate newly accessible repositories and changed paths, record source commit and content hash, distinguish complete from sampled review, and maintain per-source receipts privately. Review remaining user-authored history in bounded resumable batches with explicit disposition and sensitivity. Promote only durable, evidence-supported preferences after reconciling later corrections. Keep stale facts and one-off permissions out of global policy. Add new rules and regression cases when a distinct gap is found.

A useful initial release can be structurally complete for its stated scope while the historical discovery and future evolution remain open. Report those boundaries each time rather than labeling a moving corpus permanently exhaustive.

## Other-agent integration checkpoint

A later fetch during this work found `0bdc6fed00c35cec4ff3bf756da9d1f9dbb0c3a9` on main. That published revision supplied 125 stable rules, task playbooks, public profile, model adapters, source references, and a Python builder/adopter with tests. Those modules were reviewed and integrated; the current user correction explicitly supersedes their Python maintenance exception. The full combined guide is built and validated in Rust. Initial Python audit/validation reports are preserved as historical records under `docs/archive/`; their pass counts do not apply to the Rust replacement. Current observed results belong in `docs/VALIDATION.md`.
