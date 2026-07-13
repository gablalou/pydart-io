# Phase 1: Core Zero-Copy Round-Trip & Interop - Context

**Gathered:** 2026-07-13
**Status:** Ready for planning

<domain>
## Phase Boundary

A user can take a simple non-null numeric/bool pandas DataFrame, convert it to an Arrow Table with true zero-copy and back, verify the copy status via a first-class diagnostics/strict-mode API, and hand the Table off to pyarrow/Polars/DuckDB via the Arrow PyCapsule Interface (and accept one back) — all zero-copy. Covers CONV-01, CONV-02, DIAG-01, DIAG-02, CAP-01, CAP-02. Broader dtype/null/string/categorical coverage is Phase 2 — out of bounds here.

</domain>

<decisions>
## Implementation Decisions

### Public API Naming
- **D-01:** The main Python-facing class is named `Table`, matching pyarrow's `pa.Table` convention — minimizes friction for the pyarrow-familiar audience this project targets.
- **D-02:** Conversion methods mirror pyarrow's naming exactly: `from_pandas` / `to_pandas` — a migrating user can often just change the import.

### Diagnostics API Shape
- **D-03:** Strict zero-copy mode raises a clear exception naming the offending column/dtype when a copy would be required, rather than silently succeeding or falling back — deliberately avoiding pyarrow's `zero_copy_only` credibility gap (a flag that's documented as rarely working even when it should).
- **D-04:** Per-column copy diagnostics are exposed via a separate, on-demand call (e.g. `table.copy_report()`), not bundled into the normal conversion return value — keeps the standard conversion path uncluttered while still making "why did this copy?" answerable.

### PyCapsule Interop Scope
- **D-05:** Phase 1's PyCapsule interop (`__arrow_c_array__`/`__arrow_c_stream__`/`__arrow_c_schema__`) must be validated against pyarrow, Polars, AND DuckDB — not just pyarrow. This directly tests the ecosystem-interop claim (CAP-01/CAP-02) rather than deferring it.

### Zero-Copy Proof Strategy
- **D-06:** Zero-copy claims (CONV-01, CONV-02) must be proven with BOTH a pointer/buffer-address identity check (same underlying memory address before/after conversion) AND an allocation-counting test (no new heap allocation for the data buffer during conversion). This is the strongest available proof and directly shapes the acceptance criteria/test suite for this phase.

### Claude's Discretion
- Crate layout (single Rust crate with a `python` feature vs. a two-crate workspace mirroring `arro3`'s core+bindings split) — a technical architecture decision, not a user-facing one. Research recommends the two-crate shape; planner should follow that unless a concrete reason emerges not to.
- Exact exception type/hierarchy for strict-mode failures, and the exact shape of the `copy_report()` return value (dict vs. dataclass vs. named object) — implementation detail within the locked decisions above.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Research (from project init)
- `.planning/research/STACK.md` — recommended stack (arrow-rs, PyO3, pyo3-arrow, maturin) and critical zero-copy caveats
- `.planning/research/ARCHITECTURE.md` — component boundaries, ownership/lifetime rules, dependency-ordered build sequence
- `.planning/research/PITFALLS.md` — FFI memory-safety, GIL discipline, and false zero-copy-claim pitfalls directly relevant to this phase
- `.planning/research/SUMMARY.md` — synthesized findings and Phase 1 rationale

### Project Context
- `.planning/PROJECT.md` — core value, constraints (Rust+Python, Arrow-format-compatible, uv-compatible tooling)
- `.planning/REQUIREMENTS.md` — CONV-01, CONV-02, DIAG-01, DIAG-02, CAP-01, CAP-02 full requirement text

</canonical_refs>

<code_context>
## Existing Code Insights

Greenfield project — no existing code, no codebase maps, nothing to reuse yet. This phase establishes the foundational patterns (crate layout, Table class shape, diagnostics API) that all later phases build on.

</code_context>

<specifics>
## Specific Ideas

- Migrating pyarrow users should be able to largely reuse existing code by swapping the import — hence `Table`, `from_pandas`, `to_pandas` naming matching pyarrow exactly.
- Strict mode should behave like a real contract (raise with a specific, actionable error), not a flag that quietly does nothing — this was called out explicitly as something to do better than pyarrow.
- Ecosystem interop (Polars, DuckDB) should be proven in Phase 1, not deferred — it's part of what makes the zero-copy claim credible.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within Phase 1 scope. (Nulls, strings, categoricals, datetime/timezone, timedelta, and multi-chunk support are already scoped to Phase 2 per ROADMAP.md, not raised as new scope here.)

</deferred>

---

*Phase: 1-core-zero-copy-round-trip-interop*
*Context gathered: 2026-07-13*
