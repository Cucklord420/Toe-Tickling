---
name: local-ai-pilots
description: Use to evaluate local LLM inference, request routing, context compression, agent memory or code-indexing projects before changing an existing local AI stack. Require platform-specific tests, raw-evidence fidelity, meaningful baseline comparisons and resource/TCO measurements.
license: AGPL-3.0-only
---

# Local AI, memory and context benchmark skill

Avoid layering new model engines, schedulers, permanent memory databases or telemetry services on top of a working local AI system without a demonstrated benefit.

## Decide what needs improvement

Identify one measurable bottleneck: answer quality, cold start, concurrent throughput, context truncation, agent-log errors, retrieval/navigation accuracy, RAM/VRAM limit, disk I/O or electricity. Record current engine/model/version, relevant OS/driver and actual hardware before claiming compatibility.

## Candidate paths

- [Colibri](https://github.com/JustVugg/colibri) advertises disk-streamed MoE/CPU/Vulkan approaches; benchmark on the actual GPU/driver/model. A Vulkan claim is **not** proof of good AMD performance.
- [vLLM](https://github.com/vllm-project/vllm) is conditional for real high-throughput demand; verify the exact ROCm/CUDA/OS/model stack.
- [Personal-AI-Router](https://github.com/NVIDIA/Personal-AI-Router) routes independent inference requests; it does **not** combine GPU memory or shard one model.
- [Headroom](https://github.com/headroomlabs-ai/headroom) and [context-mode](https://github.com/mksglu/context-mode) can be tested for context efficiency only when raw outputs remain available. Reject any compression that loses exact error codes, SHAs, citations, denial evidence or security warnings.
- [Graphify](https://github.com/Graphify-Labs/graphify) must beat simple local search on real public code-navigation tasks and satisfy local/cloud data boundaries.
- [ai-memory](https://github.com/akitaonrails/ai-memory), [Hindsight](https://github.com/vectorize-io/hindsight) and [claude-mem](https://github.com/thedotmack/claude-mem) overlap Git-backed handoffs; require a specific demonstrated need and check retention/telemetry first.

## Pilot and acceptance

1. Select one exact source revision and read license, known failures, install scripts, dependency/version constraints and data path.
2. Use small sanitized/public fixtures; isolate test disk, network, credentials and private data.
3. On identical requests and settings, measure quality and citations, first-token/total latency, throughput, power when available, peak RAM/VRAM, storage, crash/restart behavior and raw-data fidelity. Record sample size and unmeasured metrics.
4. Deliberately test malformed output, unavailable model, missing network, overlong context, cancellation and rollback.
5. Calculate one-year/five-year incremental TCO using real setup, disk, power, patching, backup and subscription assumptions; unknown is not zero.
6. Keep current runtime unless a validated adoption/change is separately authorized. An advertised benchmark or a successful process start is not a proven improvement.

No hardware purchase, cloud data upload, live migration or exposed public service is granted by this skill.

## Self-checks

Use identical sanitized tasks and exact versions to compare baseline versus candidate on quality, latency, memory, storage, power where available, privacy and TCO. Require raw error and citation fidelity, test unavailable models and network failures, and separate unrun device checks from observed success.
