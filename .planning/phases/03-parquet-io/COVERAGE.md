# API Coverage — Phase 3 (Parquet IO)

> Full coverage by default. Opt-outs are explicit, reasoned decisions.

**Determination: no external API / SDK / service integration in this phase.**

The `api-coverage` detector was re-run at plan time over both the ROADMAP Phase 3
section and `PLAN.md` scope (`node gsd-core/bin/lib/api-coverage.cjs --json`) and
returned `detected: false`. The orchestrator's earlier `detected: true` signal
matched the literal word "API" inside the CONTEXT.md subsection heading
`### Predicate Pushdown & Projection API (PARQ-04/PARQ-05)` — that heading
describes **Flint's own Python method surface** (`Table.from_parquet` /
`table.to_parquet`, the `filters=[(col, op, value)]` tuple shape), not a
third-party API, SDK, service, endpoint, OAuth flow, or webhook.

Phase 3 adds exactly one dependency: the `parquet` crate (apache/arrow-rs,
pinned `59.1.0` in lockstep with `arrow`), for **local Parquet file IO**. There
is no network call, no external service, no authenticated endpoint anywhere in
this phase's scope (D-20 locks path/`str`-only, local-filesystem-only; PROJECT.md
defers all remote/object-store IO).

No capability matrix applies because there is no external capability surface to
enumerate. This file exists so the `api-coverage.verify-pre` seal-time gate has a
reasoned record to validate against rather than re-deriving this from scratch.

| capability | decision | reason |
|---|---|---|
| (none — no external API/SDK/service) | N/A | phase integrates a local Rust crate (`parquet`), not an external service |
