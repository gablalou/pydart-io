# pydart

## What This Is

A Rust-backed Python library for Arrow-format-compatible columnar data — a leaner, lower-level alternative to pyarrow focused specifically on eliminating the memory-copy overhead of pandas <-> Arrow conversion. It's for the open source community: Python/data users who feel pyarrow's conversion overhead and want a faster, more focused interop layer rather than a full DataFrame engine like Polars.

## Core Value

Converting a pandas DataFrame to/from an Arrow Table should be zero-copy (or as close to it as physically possible) and measurably faster than pyarrow — this must work and must be provably faster, or the project has no reason to exist.

## Requirements

### Validated

- ✓ Read/write Parquet files — Phase 3 (`from_parquet`/`to_parquet`: compression codec selection, row-group sizing, statistics-driven predicate pushdown + column projection, multi-file/directory read, logical-type fidelity for tz-aware timestamps and categorical/dictionary columns)

### Active

- [ ] Zero-copy (or minimal-copy) conversion from pandas DataFrame to Arrow-compatible Table, implemented in Rust with Python bindings
- [ ] Zero-copy (or minimal-copy) conversion from Arrow-compatible Table back to pandas DataFrame
- [ ] Arrow columnar memory format compatibility (interoperates with the existing Arrow/Parquet ecosystem — Polars, DuckDB, etc.)
- [ ] Benchmark suite comparing conversion speed and memory usage directly against pyarrow

### Out of Scope

- Compute kernels (filter, groupby, join, aggregation) — that's a query-engine concern (Polars' territory), not this library's job. Revisit as a future milestone if there's demand.
- Distributed / out-of-core execution — single machine, in-memory only for the foreseeable future.
- Multi-language bindings (R, Node, etc.) — Python only. Rust core could theoretically support this later, but not a v1 concern.
- CSV/JSON file IO — Parquet is the priority; other formats deferred until Parquet path is solid.

## Context

- The user's daily pain point is pyarrow's pandas <-> Arrow round-trip: converting DataFrames to/from Arrow Tables copies data unnecessarily, which adds up at scale.
- Polars already solves Rust + Python + Arrow-compatible + fast pandas interop, but as a full DataFrame/query engine. This project deliberately stays narrower and lower-level — a bridge/interop library, not a competitor to Polars' compute engine.
- Positioned as a pyarrow alternative specifically for the interop layer, intended for public/open-source release and adoption.

## Constraints

- **Language**: Rust core with Python bindings (e.g. PyO3/maturin-style toolchain) — this is the whole premise of the project, not negotiable.
- **Format**: Must use the Arrow columnar memory format (not a custom layout) so it interoperates with the existing Arrow/Parquet ecosystem.
- **Scope discipline**: v1 is bridge + Parquet IO only — no compute engine, no distributed execution, no other language bindings.
- **Tooling**: Python packaging/dev workflow must be `uv`-compatible (install via `uv add`/`uv pip install`, lockfile and dev commands work under `uv`).

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Arrow-compatible memory format (not custom) | Interop with existing ecosystem (Parquet, Polars, DuckDB) matters more than a bespoke layout | ✓ Confirmed — Phase 1: `Table` composes `pyo3_arrow::PyTable`, PyCapsule export/import verified zero-copy against pyarrow, Polars, and DuckDB |
| Narrower/lower-level than Polars | Polars already owns the full DataFrame/query-engine niche; differentiate as a leaner interop-focused library | — Pending |
| v1 = zero-copy bridge + Parquet IO, no compute kernels | Keep v1 scope tight and provably valuable (benchmarkable) before expanding | — Pending |
| Success measured by benchmark vs pyarrow | Speed/memory claims need hard numbers, not just "should be faster" | — Pending (Phase 4) |
| Genuine zero-copy numpy borrow implemented by hand (not pyo3-arrow's `from_numpy()`) | pyo3-arrow's `from_numpy()` was found to copy via `PrimitiveArray::from_iter_values` even on its contiguous fast path — reading its source was necessary to catch this | ✓ Confirmed — Phase 1: pointer-identity proof + Rust allocation-counter proof both pass, forward and reverse |
| Multi-chunk `from_pandas` truncation (CR-01) fixed via `arrow::compute::concat`, single-chunk path unchanged | Silent data loss on an ordinary `pd.concat` input was a credibility-breaking bug; the fix must not regress the certified single-chunk zero-copy path | ✓ Confirmed — Phase 1 re-verification: 6-row/2-chunk round-trip fixed, no regression to D-06 pointer-identity proof |
| DIAG-01/DIAG-02 multi-chunk diagnostics-honesty gap deferred to Phase 2 (CONV-08) via recorded override, not fixed in Phase 1 | Root cause (`plan_column` has no chunk-count visibility) is the same mechanism CONV-08 needs to solve anyway; bundling avoids a throwaway patch | ✓ Confirmed — Phase 2 Plan 05: resolved via Strategy B (post-hoc `ColumnConversionRecord` correction), `strict=True` now correctly rejects multi-chunk columns |
| `classify_dtype` restructured from `dtype.kind`-first to isinstance-first dispatch | Correctly distinguishing ArrowDtype/ExtensionDtype/numpy backends requires type identity, not just the dtype's numpy-compatibility `.kind` character — the `dtype.kind`-first approach couldn't honestly reject masked extension dtypes | ✓ Confirmed — Phase 2 Plan 01: foundation every subsequent dtype-family slice (string, categorical, datetime/tz, timedelta) extends |
| Categorical round-trip fidelity requires two independent fixes, not one | `ordered` flag lives on Arrow `Field` (not `DataType`), and pandas `ArrowDtype` types_mapper is too blunt for dictionary columns — a single fix could not address both the import-side and export-side metadata loss | ✓ Confirmed — Phase 2 Plan 03: `Field::new_dictionary`+`with_dict_is_ordered` (import) and a per-column-type-aware `types_mapper` closure (export) |
| Row-group-level pruning is an optimization layered strictly under exact row-level filtering, never a replacement for it | Statistics-driven skip decisions can only prove "no match possible" (conservative); only `RowFilter`/`ArrowPredicateFn` per-row evaluation guarantees zero false positives, so both must derive from one parsed `FilterExpr` list, never re-derived independently | ✓ Confirmed — Phase 3 Plan 03: `surviving_row_groups` isolated skip-engagement proof + 36-case six-operator boundary property test; a related CR-01 code-review finding (silent row-drop on out-of-range integer filter casts) was fixed and independently re-verified |
| Categorical `.cat.categories` order and unused-category retention are NOT preserved through a Parquet round-trip | Confirmed arrow-rs `ArrowWriter`/`DictEncoder` limitation (reassigns dictionary keys in first-occurrence-during-encoding order) with no `WriterProperties` fix in parquet 59.1.0; pyarrow does not share this limitation. Accepted rather than hand-rolling a Parquet column writer (against "Don't Hand-Roll" guidance) | ✓ Accepted — Phase 3 Plan 04, user checkpoint decision; regression-pinned by `test_ordered_categorical_category_order_not_guaranteed_known_gap`; flag in Phase 4 release docs if categorical fidelity is a headline interop claim |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-07-24 after Phase 3*
