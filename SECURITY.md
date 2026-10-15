# Security and private information

Never disclose a credential, private record, precise location, or exploitable private-system detail in a public issue or pull request. If GitHub private vulnerability reporting is enabled, use it; its availability is not assumed. Otherwise report a non-sensitive summary to the owner and request a private channel without posting the underlying private data.

The validator detects selected secret patterns, private file paths, broken local links, invalid IDs, and generated-file drift. It does not detect every private fact or semantic prompt injection. Human/model review remains necessary. The guidebook is instructions, not a sandbox or enforcement guarantee.

If sensitive content enters Git history, removing the latest file is insufficient. Stop redistribution, notify the owner without repeating the secret, and assess revocation/rotation and history cleanup under the appropriate authorization.

CI runs without write permissions, custom secrets, or public self-hosted runners. External Actions are pinned to full commits. Keep permissions and pins reviewed when changing the workflow.
