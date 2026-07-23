# Phase 3: Parquet IO - Context

**Gathered:** 2026-07-23
**Status:** Ready for planning

<domain>
## Phase Boundary

A user can read and write Parquet files directly against Flint's `Table` (the Arrow core established in Phases 1-2) — not via pandas directly. Covers PARQ-01 through PARQ-06: single-file and multi-file/directory read into a `Table`, write with a chosen compression codec and row-group size, row-group statistics enabling predicate pushdown, column projection + row-level-exact predicate filtering on read, and correct round-trip of the full dtype range (including tz-aware timestamps and categorical/dictionary encoding) established in Phases 1-2. Benchmarking (Phase 4) and packaging (Phase 4) are out of bounds here. Compute kernels beyond the fixed comparison-operator filter set (no groupby/join/aggregation) remain out of scope per PROJECT.md.

</domain>

<decisions>
## Implementation Decisions

### API Surface (PARQ-01/PARQ-02)
- **D-19:** Parquet read/write follow the same `Table.from_X`/`table.to_X` naming pattern already locked for pandas conversion (D-01/D-02): `Table.from_parquet(...)` (classmethod) and `table.to_parquet(...)` (instance method). No separate `flint.read_parquet`/`flint.write_parquet` module-level functions — one consistent naming pattern across the whole `Table` API.
- **D-20:** `Table.from_parquet`/`table.to_parquet` accept `str`/`pathlib.Path` only — no file-like objects (`io.BytesIO`, open file handles) in v1. Matches PARQ-01's exact wording and keeps scope to local filesystem, consistent with PROJECT.md's deferral of remote/object-store IO.
- **D-21:** `Table.from_parquet` also supports reading multiple files as one `Table` — accepting a list of paths or a directory path, concatenated into a single `Table`. Exact mechanism (explicit list vs directory auto-discovery, schema-mismatch handling across files) is left to Claude's/planner's discretion (see below).
- **D-22:** `table.to_parquet` overwrites the target file silently if it already exists — no `overwrite=True` guard flag, matching typical write-file semantics and pyarrow's own `write_table` default behavior.

### Predicate Pushdown & Projection API (PARQ-04/PARQ-05)
- **D-23:** Read-side filtering uses pyarrow-style tuple filters: `filters=[("col", ">", 5), ("col2", "==", "x")]` — a flat list of `(column, operator, value)` tuples. No general expression-evaluator/compute engine; this stays a fixed, closed operator set (see D-25), consistent with PROJECT.md's "no compute engine" scope constraint.
- **D-24:** Multiple filter conditions in the list combine with AND only (no OR support, no nested list-of-lists/disjunctive-normal-form). Matches pyarrow's original (pre-2.0) `filters` kwarg shape, not its current DNF shape.
- **D-25:** Supported operators in v1: `==`, `!=`, `<`, `<=`, `>`, `>=`. No `in`/membership operator in v1.
- **D-26:** "Predicate pushdown" means BOTH row-group-level skipping (via written row-group statistics — the actual IO optimization, PARQ-04) AND exact row-level filtering of rows within surviving row groups (via arrow-rs's `RowFilter`/`ArrowPredicate` machinery) — the `Table` returned by `from_parquet(..., filters=...)` contains ONLY matching rows, no false positives requiring a second filter pass by the caller.
- **D-27:** Column projection (`columns=[...]`) and `filters` are independent, combinable parameters on `Table.from_parquet` — projecting to a subset of columns and filtering by predicate can be used together in the same call.

### Compression & Row-Group Defaults (PARQ-02/PARQ-03)
- **D-28:** Default compression codec when unspecified on `to_parquet` is **snappy** — matches pyarrow's own default, least surprising for direct file-size/speed comparison in Phase 4's benchmark suite.
- **D-29:** Exactly the four codecs named in PARQ-02 are supported: snappy, zstd, gzip, uncompressed. No `lz4` or other codecs added in v1 even though arrow-rs's parquet crate supports them for free — stick to the locked requirement text.
- **D-30:** Default row-group size when unspecified is a **row-count threshold** (~1,048,576 rows/group, matching pyarrow's own default), not a byte-size threshold. `to_parquet` accepts a `row_group_size` parameter (row-count) per PARQ-03 to let the user override this.

### WR-01 Nullability Fix (carried forward from 02-REVIEW.md, direct PARQ-06 dependency)
- **D-31:** Fix WR-01 as part of Phase 3, not deferred and not as a separate quick task first. `build_field` in `crates/flint-python/src/pandas.rs` currently derives Arrow field nullability from the current batch's observed `null_count() > 0` rather than the source pandas dtype's declared nullability — a nullable `int64[pyarrow]` column with zero nulls round-trips as a `not null` Flint schema field. This directly threatens PARQ-06 ("Parquet round-trip preserves logical types correctly"): a nullable-but-currently-all-non-null column written to Parquet and read back would carry the wrong (non-nullable) schema, which can break downstream `pyarrow.concat_tables`-style schema merges exactly as described in `02-REVIEW.md`. Bundling this fix into Phase 3 avoids writing PARQ-06 schema-fidelity tests against a known-broken nullability signal.

### Claude's Discretion
- Exact multi-file/directory read mechanism for D-21 (explicit `List[str|Path]` parameter vs directory-path auto-discovery vs both) and the schema-mismatch policy across files (strict-match-required error vs best-effort union) — implementation detail within the locked "multi-file read is in scope" decision.
- Exact fix mechanism for WR-01 (D-31) — e.g., whether the source dtype's declared nullability is threaded through at `classify_dtype`/`plan_column` time or captured elsewhere in the `from_pandas` pipeline — implementation detail within the locked "fix in Phase 3" requirement.
- Exact row-group statistics written (min/max only vs min/max + null-count + distinct-count) beyond what PARQ-04 requires for predicate pushdown to function — implementation detail, follow arrow-rs parquet crate defaults unless a concrete reason emerges not to.
- Whether categorical/dictionary columns get forced dictionary-encoding in the Parquet writer or rely on the writer's own heuristics — technical detail within PARQ-06's locked "preserve categorical/dictionary encoding" fidelity requirement.
- Internal implementation of row-level exact filtering (D-26) — e.g., building `ArrowPredicateFn` closures per operator from the fixed D-25 operator set — technical detail within the locked API contract.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Research (from project init)
- `.planning/research/STACK.md` — recommended stack; parquet crate (apache/arrow-rs, pinned in lockstep with `arrow` 59.1.0 per Version Compatibility table) is the parquet implementation to add
- `.planning/research/ARCHITECTURE.md` — component boundaries, ownership/lifetime rules Phase 3's new read/write code paths must respect
- `.planning/research/PITFALLS.md` — prior pitfalls (false zero-copy claims, single-decision-point pattern) that inform how Parquet IO code should be structured relative to existing `plan_column`/diagnostics machinery
- `.planning/research/SUMMARY.md` — synthesized findings

### Project Context
- `.planning/PROJECT.md` — core value, constraints (Rust+Python, Arrow-format-compatible, uv-compatible tooling, "no compute engine" scope boundary directly relevant to D-23's fixed filter operator set)
- `.planning/REQUIREMENTS.md` — PARQ-01 through PARQ-06 full requirement text
- `.planning/STATE.md` — Blockers/Concerns section: WR-01 (D-31, this phase) and WR-02 (numpy CoW zero-copy-borrow guarantee gap — noted as a Phase 3 blocker but NOT part of this phase's domain; it concerns the numpy buffer-borrow path, not Parquet fidelity, and is not addressed by any decision above)
- `.planning/phases/02-full-dtype-structural-coverage/02-REVIEW.md` — full WR-01 and WR-02 write-ups; read WR-01's finding in full before implementing the D-31 fix
- `.planning/phases/02-full-dtype-structural-coverage/02-CONTEXT.md` — Phase 2's locked dtype-fidelity decisions (D-07 through D-18) that PARQ-06's round-trip must preserve through a Parquet write/read cycle, not just a Table<->pandas cycle
- `.planning/phases/01-core-zero-copy-round-trip-interop/01-CONTEXT.md` — Phase 1's D-01/D-02 naming convention that D-19 extends to Parquet IO

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/flint-python/src/table.rs` (`Table` struct, composes `pyo3_arrow::PyTable`) — `Table::from_parquet`/`to_parquet` should follow the same shape as the existing `from_pandas`/`to_pandas` `#[pymethods]`: a `#[classmethod]` for read, an instance method for write, both delegating to a `pyo3`-free implementation in `flint-core` where possible (mirroring how `from_pandas`'s heavy lifting lives in `crate::pandas`, not directly in `table.rs`).
- `crates/flint-core/src/table.rs` — `flint-core`'s `Table` is currently a thin re-export of `arrow::record_batch::RecordBatch`. Parquet read/write logic that doesn't need `pyo3` (schema/row-group/predicate handling) belongs here, consistent with the existing `flint-core`/`flint-python` split (pyo3-free core, pyo3-only bindings crate).
- `crates/flint-python/src/error.rs` (`FlintError`) — new Parquet-specific error variants (file-not-found, schema-mismatch-across-files, unsupported-filter-operator) should extend this existing error type rather than introducing a parallel error path, per the established "no silent best-effort behavior, named specific errors" pattern from Phases 1-2.
- `crates/flint-python/src/pandas.rs` (`build_field`, referenced by WR-01/D-31) — the fix site for D-31; this function is also structurally the thing PARQ-06's schema round-trip depends on being correct.

### Established Patterns
- Single source-of-truth decision function pattern (`plan_column` for pandas conversion) — Parquet IO should follow the same discipline: one function determines row-group-skip/row-filter decisions, consumed identically by whatever surfaces "why was this row-group skipped" diagnostics (if any are added) and the actual read path. Don't re-derive filtering logic in two places (RESEARCH.md Pitfall 2 precedent).
- Named, specific errors for unsupported input — no silent copy or best-effort coercion (established in `classify_dtype`'s rejection pattern, D-11's object-dtype validation, D-15's non-ns temporal rejection). Applies to unsupported filter operators, multi-file schema mismatches, and unsupported compression codec strings.
- `Cargo.toml` workspace already pins `arrow = "59.1.0"` in both crates — the `parquet` crate must be added at the same `59.1.0` version (lockstep pinning, per `.claude/CLAUDE.md` Version Compatibility table) to both `flint-core` and `flint-python` (or just `flint-core` if the pyo3-python layer only needs `flint-core`'s re-exports — a planning-time call).

### Integration Points
- `crates/flint-python/src/lib.rs` (`_flint` `#[pymodule]` function) — no new top-level module functions needed given D-19 (no `flint.read_parquet`/`write_parquet`); the new `#[pymethods]` on the existing `Table` class register automatically once added to `table.rs`.
- `python/flint/__init__.py` (not yet inspected in depth, re-exports `Table` from the compiled extension) — no new Python-level re-exports needed beyond what already re-exports `Table` itself, since the new methods live on `Table`.

</code_context>

<specifics>
## Specific Ideas

- The project's established "match pyarrow naming where it reduces migration friction" posture (D-01/D-02) was extended directly to Parquet IO: `Table.from_parquet`/`to_parquet` naming, snappy default codec, ~1M-row default row-group size, and pyarrow's original (pre-DNF) tuple-filter shape were all chosen specifically to minimize surprise for a pyarrow-familiar user and to keep Phase 4's benchmark comparisons apples-to-apples.
- The project's zero-copy/diagnostics-honesty stance (Phase 1 D-03/D-04, reinforced by the DIAG-01/DIAG-02 override in Phase 1-2) surfaced again here: predicate pushdown was explicitly resolved toward exact row-level filtering (D-26) rather than a coarser row-group-only pruning that could silently return non-matching rows — consistent with the project's "never silently return something the user didn't ask for" pattern.
- WR-01 was treated as directly in-scope rather than a tangential bug, specifically because it threatens the one new acceptance criterion (PARQ-06 schema fidelity) this phase is adding — the user explicitly connected the dots rather than treating it as generic tech debt.

</specifics>

<deferred>
## Deferred Ideas

- **WR-02 (numpy Copy-on-Write zero-copy-borrow guarantee gap)** — noted as a Phase 3 blocker in STATE.md, but out of this phase's domain (concerns the numpy buffer-borrow path from Phase 1, not Parquet IO). Not discussed here; remains an open blocker to revisit, possibly alongside Phase 4's benchmark/release-readiness work if it becomes release-blocking.
- **`in`/membership filter operator** (raised during the Predicate pushdown discussion) — explicitly deferred from v1's fixed operator set (D-25). Revisit if user demand emerges post-Phase-3, particularly for filtering categorical/dictionary columns by a set of values.
- **OR / disjunctive-normal-form filter combination** (raised during the Predicate pushdown discussion) — explicitly deferred; D-24 locks AND-only for v1. Revisit if a concrete use case emerges.
- **File-like object / in-memory buffer support for Parquet read/write** (raised during the API surface discussion) — explicitly deferred; D-20 locks path/str-only for v1. Revisit if remote/streaming IO becomes a priority (currently out of scope per PROJECT.md's local-filesystem-only stance).

</deferred>

---

*Phase: 3-parquet-io*
*Context gathered: 2026-07-23*
