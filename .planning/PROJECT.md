# Flint (placeholder name)

## What This Is

A Rust-backed Python library for Arrow-format-compatible columnar data — a leaner, lower-level alternative to pyarrow focused specifically on eliminating the memory-copy overhead of pandas <-> Arrow conversion. It's for the open source community: Python/data users who feel pyarrow's conversion overhead and want a faster, more focused interop layer rather than a full DataFrame engine like Polars.

## Core Value

Converting a pandas DataFrame to/from an Arrow Table should be zero-copy (or as close to it as physically possible) and measurably faster than pyarrow — this must work and must be provably faster, or the project has no reason to exist.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Zero-copy (or minimal-copy) conversion from pandas DataFrame to Arrow-compatible Table, implemented in Rust with Python bindings
- [ ] Zero-copy (or minimal-copy) conversion from Arrow-compatible Table back to pandas DataFrame
- [ ] Arrow columnar memory format compatibility (interoperates with the existing Arrow/Parquet ecosystem — Polars, DuckDB, etc.)
- [ ] Read/write Parquet files
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
*Last updated: 2026-07-23 after Phase 2*
