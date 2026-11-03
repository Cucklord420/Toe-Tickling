---
name: desktop-automation-pilots
description: Use when evaluating offline dictation, private remote desktop, authorized web extraction, headless browsers or targeted UI-review workflows from GitHub stars. Compare existing tools and test security, permissions, actual OS fit, privacy and rollback before adoption.
license: AGPL-3.0-only
---

# Safe desktop and browser pilot selection

Use a narrow tool for a real need, not an always-on broad agent platform. This skill identifies candidates; it is not an installer.

## Offline dictation

Compare [Handy](https://github.com/cjpais/Handy) with [Hex](https://github.com/anomalyco/hex). Verify exact supported release/OS, microphone permissions, offline behavior, hotkeys, latency, accuracy on harmless scripted speech, CPU/power, correction workflow and actual outbound connections. Prefer one tested workflow rather than installing redundant background recorders. Do not test with customer audio or private transcripts.

## Private remote access

Consider [RustDesk](https://github.com/rustdesk/rustdesk) only if existing private administration lacks a necessary interactive desktop feature. Verify authentication, relay/rendezvous, unattended access, updates, permissions, private network routing, recovery and revocation. Never expose ports publicly or enable persistent remote control under a research request.

## Authorized browser automation

Evaluate [Scrapling](https://github.com/D4Vinci/Scrapling) or [Lightpanda](https://github.com/lightpanda-io/browser) using owned test pages or expressly permitted public websites. Check site terms/robots where applicable, rate limiting, DOM/JS compatibility, provenance, malformed pages, timeouts, fallback to a normal browser, and resource use. Do not bypass access controls or antiexploitation protections.

## Optional UI/code tools

For an actual interface task, borrow focused keyboard/accessibility, focus, typography and responsive checks from [Hallmark](https://github.com/Nutlope/hallmark) or [jakubkrehel/skills](https://github.com/jakubkrehel/skills). For actual code-navigation needs, trial [Graphify](https://github.com/Graphify-Labs/graphify) against simple local search with data-egress review. No speculative redesign or database install.

## Pilot gate

Read current source/license/dependencies, confirm target platform, preserve baseline, use only owned disposable fixtures, test success/denial/malformed data/interruption/cleanup, compare reliability and maintenance/TCO, and document rollback. Label each outcome **adopt / conditional pilot / retain existing / reject** with actual evidence. Do not buy, install on employer devices, grant new credentials, deploy, or claim unsupported compatibility.
