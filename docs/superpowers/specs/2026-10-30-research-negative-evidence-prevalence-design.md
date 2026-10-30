# Research negative evidence and prevalence design

## Goal

Make research summaries distinguish material negative evidence from duplicated, anecdotal, stale, or resolved reports, and prevent unsupported complaint counts from being presented as failure rates.

## Design

Add one stable standing requirement, `RES-013`, to the canonical `rules/20-research.md`. For relevant product, software, service, and workflow research, actively seek credible negative experiences, failure modes, hidden restrictions, and practical downsides in relevant issue trackers, original user reports, independent communities, and primary or official sources as appropriate. Preserve existing rule IDs and limit the requirement to relevant research; it must not imply that every review site must be searched on every task. For material negative findings, report the supported time window and distinct corroborated report count where defensible, and state a denominator or sample-selection method only when evidence supports it. Never infer an incidence rate from unrepresentative reviews or complaints. Discuss material unresolved downsides with their practical consequences and reassess earlier recommendations when revisited.

Add two semantic scenarios: repeated quotations of one defect count as one report, while two serious independent unresolved data-loss reports remain material even when no representative denominator exists. Neither case claims real-world model adoption.

Extend the Rust standing-rule family gate to require `RES-013`, with a focused regression proving removal is rejected. Regenerate the guide and integrity records from canonical sources, update the version and changelog, and retain the existing verification register and history.

## Scope and limits

The rule and examples specify expected behavior; mechanical Rust checks can enforce that the rule exists and generated artifacts agree, but cannot prove that an agent searches well or follows the rule. Adoption evidence remains a separate question. No exhaustive review-site sweep is required, and no failure rate may be invented where representative denominator data is unavailable.

## Acceptance

- `RES-001` through `RES-012` remain unchanged and active; `RES-013` is added with relevant negative-evidence discovery, prevalence limits, and downside reassessment.
- Scenarios cover duplicate quotations and serious unresolved findings without a denominator.
- Rust validation rejects removal of `RES-013`.
- Generated GUIDEBOOK, adapters, lock, and reading manifest match canonical source state.
- Formatting, Clippy, workspace tests, builder check, and exact-head CI are attempted; results are reported only when observed.
