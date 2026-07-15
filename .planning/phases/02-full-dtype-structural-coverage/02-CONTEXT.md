# Phase 2: Full Dtype & Structural Coverage - Context

**Gathered:** 2026-07-15
**Status:** Ready for planning

<domain>
## Phase Boundary

Extend Phase 1's numeric/bool-only pandas<->Arrow conversion pipeline (`plan_column`, `from_pandas`, `to_pandas`, strict mode, `copy_report()`) to handle every realistic pandas column shape: nulls, object/string, categorical, datetime/timezone, timedelta, and multi-chunk tables. Covers CONV-03, CONV-04, CONV-05, CONV-06, CONV-07, CONV-08. Also closes the carried-forward DIAG-01/DIAG-02 diagnostics-honesty gap for multi-chunk columns (recorded override from `01-VERIFICATION.md`, accepted by John Columna 2026-07-15). Parquet IO, benchmarking, and packaging are out of bounds here (Phases 3-4).

</domain>

<decisions>
## Implementation Decisions

### Null Handling Scope (CONV-03)
- **D-07:** Only `pandas.ArrowDtype`-backed nullable columns are in scope for Phase 2's null support (e.g. `int64[pyarrow]` with nulls). These already carry an Arrow-compatible null bitmap internally via the existing `DtypeBackend::Arrow` path.
- **D-08:** Pandas' own nullable extension dtypes (`Int64`, `boolean`, `Float64` — capital-letter masked-array dtypes using `pd.NA`) are explicitly OUT of scope for Phase 2. Not rejected as an error necessarily, but not a locked requirement — planner should confirm current `classify_dtype` rejection behavior for these is still an honest, clear error.
- **D-09:** Plain numpy `float64` columns containing `NaN` are NOT treated as nulls. They keep going through the existing Phase 1 zero-copy numeric path completely unchanged — `NaN` round-trips as a literal float value with no Arrow null bitmap involved. This is a deliberate non-change: CONV-03's null-handling work targets only ArrowDtype-backed nullable columns, not NaN-as-missing-value semantics on the numpy path.

### String/Object Dtype Scope (CONV-04)
- **D-10:** Both `string[pyarrow]`/ArrowDtype string columns AND legacy numpy `object`-dtype string columns are in scope. Object-dtype columns are accepted with an honest copy — introspection + conversion — reported via `copy_report()`/`RequiresCopy` (reason: no Arrow-compatible physical layout), not silently or rejected outright.
- **D-11:** When an object-dtype column is accepted, Flint MUST validate that every non-null value is a `str` (or the value is `None`/null). If any non-string value is found (int, dict, custom object, etc.), raise a clear error naming the column and the offending value's type — no best-effort `str()` coercion. This matches the project's existing explicit-rejection philosophy (no silent best-effort behavior).

### Multi-Chunk Handling Strategy (CONV-08 + DIAG-01/DIAG-02 override closure)
- **D-12:** Phase 2 does NOT pursue genuine zero-copy multi-chunk preservation. Keep the Phase 1 approach of concatenating a multi-chunk Arrow-backed pandas column into one contiguous batch via `arrow::compute::concat` (an honest copy, not zero-copy). This is a deliberate scope decision — true zero-copy multi-chunk support would require per-column independent chunk-boundary alignment (pyarrow's `ChunkedArray` model allows different chunk boundaries per column; arrow-rs `RecordBatch` requires uniform row counts per batch) and is judged too large a lift for this phase.
- **D-13:** The actual Phase 2 fix is making `plan_column`/`ColumnConversionRecord` chunk-count-aware, so `strict=True` and `copy_report()` honestly reflect the concat copy for multi-chunk columns — closing the DIAG-01/DIAG-02 gap recorded as an override in `01-VERIFICATION.md`.
- **D-14:** This is an intentional behavior change for existing callers: once `plan_column` becomes chunk-count-aware, `from_pandas(df, strict=True)` will now RAISE `ZeroCopyRequiredError` for a multi-chunk Arrow-backed column that previously succeeded silently (because the concat is now correctly recognized as a real copy). This is the correct fix per DIAG-01's contract ("errors instead of silently falling back to a copy") — no opt-in flag (e.g. `allow_rechunk`) to bypass this under strict mode.

### Datetime/Timezone/Timedelta (CONV-06/CONV-07)
- **D-15:** Only nanosecond-resolution pandas dtypes are in scope: `datetime64[ns]`, `datetime64[ns, tz]`, and `timedelta64[ns]`, mapping directly to Arrow `Timestamp(Nanosecond, tz)` / `Duration(Nanosecond)`. Non-ns-resolution columns (`datetime64[s]`/`[ms]`/`[us]`, possible since pandas 2.0's unit-flexible dtypes) are explicitly OUT of scope and should be rejected with a clear error naming the column and its actual resolution.
- **D-16:** Timezone-aware timestamp columns round-trip the tz string exactly as-is (e.g. `"America/New_York"`) with no internal normalization to UTC. Flint does not hand-roll DST/ambiguous-time logic — it trusts pandas'/arrow-rs's own tz handling and only surfaces an error if arrow-rs itself rejects the input.

### Categorical Round-Trip Fidelity (CONV-05)
- **D-17:** Both the `ordered` flag and the exact category order must be preserved exactly across the round trip. Reconstructing a Table back to pandas must yield a `Categorical` with identical `ordered` and `categories` (order included) to the original — not just equivalent values. This applies even for unordered categoricals, where category definition order still matters for round-trip equality.
- **D-18:** The exact integer code width pandas chose (`int8`/`int16`/`int32`/`int64`, based on category count) must be preserved across the round trip — the reconstructed `Categorical`'s `.cat.codes.dtype` must match the source, not be normalized to one fixed width.

### Claude's Discretion
- Exact rejection error message/type for pandas nullable extension dtypes (`Int64`/`boolean`/`Float64`) and non-ns-resolution datetime/timedelta dtypes — should follow the existing `FlintError::UnsupportedColumn` pattern from Phase 1, naming the column and dtype.
- Internal representation details for preserving categorical code width and order (e.g. how `classify_dtype`/`plan_column` are extended) — implementation detail within the locked fidelity requirements above.
- Whether/how `ColumnConversionRecord`'s `reason` field gains new categories to distinguish "structural copy due to multi-chunk" from "dtype-driven copy" (e.g. object-dtype string) — API detail within D-13's locked chunk-count-awareness requirement.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Research (from project init)
- `.planning/research/STACK.md` — recommended stack and critical zero-copy caveats
- `.planning/research/ARCHITECTURE.md` — component boundaries, ownership/lifetime rules
- `.planning/research/PITFALLS.md` — Pitfall 3 (object dtype / string column zero-copy trap) is directly load-bearing for D-10/D-11; also covers null-bitmap and chunking pitfalls relevant to D-07-D-09 and D-12-D-14
- `.planning/research/SUMMARY.md` — synthesized findings

### Project Context
- `.planning/PROJECT.md` — core value, constraints, Key Decisions table (includes the CR-01 fix and the DIAG-01/DIAG-02 deferral rationale)
- `.planning/REQUIREMENTS.md` — CONV-03 through CONV-08 full requirement text
- `.planning/phases/01-core-zero-copy-round-trip-interop/01-VERIFICATION.md` — the recorded override for the DIAG-01/DIAG-02 multi-chunk diagnostics gap that D-12-D-14 directly resolve; read the full "Gaps Summary" section before planning the `plan_column` chunk-count-awareness fix
- `.planning/phases/01-core-zero-copy-round-trip-interop/01-CONTEXT.md` — Phase 1's locked decisions (D-01 through D-06) that Phase 2 extends, not replaces

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/flint-core/src/pandas_plan.rs` (`plan_column`, `DtypeBackend`, `ArrowKind`, `ColumnPlan`) — the single source-of-truth decision matrix both `from_pandas` and diagnostics consume. Phase 2 extends this matrix with new `ArrowKind`/backend variants (nulls, strings, categorical, temporal) rather than duplicating decision logic elsewhere. Currently `pyo3`-free (no Python dependency) — new variants should preserve that so the matrix stays unit-testable without a Python interpreter.
- `crates/flint-python/src/pandas.rs` (`classify_dtype`, `from_pandas`, `import_column_via_pandas_stream`, `borrow_numpy_numeric_column`) — the per-column conversion driver. `classify_dtype` currently rejects any non-numeric/bool `dtype.kind` with `FlintError::UnsupportedColumn`; Phase 2 broadens this classification, and any dtype still out of scope (D-15's non-ns temporal, D-08's nullable extension types) should follow this same explicit-rejection pattern.
- `crates/flint-python/src/diagnostics.rs` (`check_strict`, `build_copy_report`) — consumes `ColumnConversionRecord`s from `pandas.rs`; D-13's chunk-count-awareness fix flows through here without needing new consumer-side logic, provided `plan_column`'s output correctly reflects the chunk-count-aware decision.
- `crates/flint-python/src/table.rs` (`Table::to_pandas`) — already iterates `batches().to_vec()` (not just the first batch) when reconstructing a pandas DataFrame, so the `to_pandas` direction of CONV-08 (multi-batch Table -> pandas) already appears structurally correct; only the `from_pandas` direction's concat-then-diagnose gap (D-12/D-13) needs work.

### Established Patterns
- Every per-column copy-vs-borrow decision flows through exactly one function (`plan_column`) consumed identically by both the conversion path and the diagnostics path — never re-derive the decision in two places (RESEARCH.md Pitfall 2 / apache/arrow#39194). Phase 2 must extend this same single-decision-point pattern, not add a parallel matrix for new dtypes.
- Unsupported dtypes are rejected with a named, specific error (`FlintError::UnsupportedColumn`) — never a silent copy or best-effort coercion. D-11's object-dtype content validation and D-15's non-ns temporal rejection both follow this existing pattern.
- Zero-copy paths avoid hand-rolled FFI/marshalling wherever pyarrow's/pandas' own `__arrow_c_stream__` export or pyo3-arrow's `PyTable` machinery can be delegated to instead (see `import_column_via_pandas_stream`, `to_pandas`'s `into_pyarrow` + `to_pandas(types_mapper=...)` composition).

### Integration Points
- New `ArrowKind`/`DtypeBackend` variants in `pandas_plan.rs` plug into `classify_dtype`'s `dtype.kind` match arm and `plan_column`'s match arms — this is the primary extension point for nulls, strings, categoricals, and temporal types.
- `import_column_via_pandas_stream`'s multi-batch branch (currently unconditional `concat`) is where D-13's chunk-count-awareness needs to surface its result back into a `ColumnConversionRecord`, likely requiring `from_pandas` to learn the batch count before finalizing the per-column plan (today the plan is computed before the stream is read at all — this ordering itself is the root cause noted in `01-VERIFICATION.md`).

</code_context>

<specifics>
## Specific Ideas

- The project's zero-copy honesty stance (established in Phase 1's D-03/D-04 and reinforced by the DIAG-01/DIAG-02 override) directly shaped several Phase 2 decisions: object-dtype strings are accepted but honestly labeled as a copy (D-10), multi-chunk columns are accepted but honestly labeled as a copy (D-12/D-13), and strict mode is expected to correctly reject what it should reject even when that changes prior (buggy) observed behavior (D-14).
- Categorical fidelity (D-17/D-18) was explicitly resolved toward full fidelity (exact `ordered`/category order, exact code width) rather than "values only" — consistent with the project's broader precision-over-convenience posture.
- Scope discipline was exercised on two fronts: nullable extension dtypes (D-08) and non-ns temporal resolution (D-15) were both explicitly excluded rather than silently attempted, to keep Phase 2 focused on the ArrowDtype-first, ns-first pandas idioms that are both most common and cleanest to support correctly.

</specifics>

<deferred>
## Deferred Ideas

- **Genuine zero-copy multi-chunk preservation** (raised during the Multi-chunk handling strategy discussion) — explicitly deferred, not to a specific future phase but noted as a known, larger architectural undertaking (per-column independent chunk-boundary alignment) that could be revisited post-v1 if the honest-copy fallback proves insufficient in practice.
- **Pandas nullable extension dtypes** (`Int64`/`boolean`/`Float64` masked arrays) — deferred, not scheduled to any specific future phase. Revisit if user demand emerges post-v1.
- **Non-nanosecond datetime/timedelta resolution** (`datetime64[s]`/`[ms]`/`[us]`) — deferred, not scheduled to any specific future phase. Revisit if user demand emerges post-v1.

None — discussion stayed within Phase 2 scope; the three items above are dtype/structural variants explicitly excluded from THIS phase's scope by decision, not new capabilities belonging to a different phase.

</deferred>

---

*Phase: 2-full-dtype-structural-coverage*
*Context gathered: 2026-07-15*
