---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
current_phase: 03
current_phase_name: parquet-io
status: executing
stopped_at: Completed 03-01-PLAN.md
last_updated: "2026-07-23T12:54:12.322Z"
last_activity: 2026-07-23
last_activity_desc: Phase 03 execution started
progress:
  total_phases: 3
  completed_phases: 2
  total_plans: 14
  completed_plans: 11
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-07-15)

**Core value:** Converting a pandas DataFrame to/from an Arrow Table should be zero-copy (or as close to it as physically possible) and measurably faster than pyarrow — this must work and must be provably faster, or the project has no reason to exist.
**Current focus:** Phase 03 — parquet-io

## Current Position

Phase: 03 (parquet-io) — EXECUTING
Plan: 2 of 4
Status: Ready to execute
Last activity: 2026-07-23 — Phase 03 execution started

Progress: [████████░░] 79%

## Performance Metrics

**Velocity:**

- Total plans completed: 10
- Average duration: - min
- Total execution time: 0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01 | 5 | - | - |
| 02 | 5 | - | - |

**Recent Trend:**

- Last 5 plans: -
- Trend: -

*Updated after each plan completion*
| Phase 01 P01 | 24min | 2 tasks | 13 files |
| Phase 01 P02 | 35 | 2 tasks | 12 files |
| Phase 01 P03 | 20min | 2 tasks | 4 files |
| Phase 01 P04 | 10min | 2 tasks | 5 files |
**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase quick P260715-smf | 12min | 2 tasks | 2 files |
| Phase 03 P01 | 40min | 3 tasks | 6 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- Roadmap: Vertical MVP slicing chosen over research's horizontal-layer suggestion — Phase 1 delivers a narrow but complete numeric-only round-trip (conversion + strict-mode diagnostics + PyCapsule interop) before broadening dtype coverage in Phase 2, per project_mode=mvp.
- Roadmap: Benchmarking (BENCH-01/02) and packaging (PKG-01/02/03) combined into a single Phase 4 "Benchmark & Release Readiness" phase — both are release-gating validation concerns for the core value claim, coarse granularity favors combining them rather than two thin phases.
- [Phase 01 P01]: Raised PyO3 abi3 floor from abi3-py39 to abi3-py311: pyo3-arrow 0.19.0's buffer-protocol methods require CPython stable-ABI buffer support (>=3.11)
- [Phase 01 P01]: Set pyproject.toml requires-python to >=3.12 to satisfy the RESEARCH.md-pinned numpy==2.5.1 dev dependency under uv's resolver
- [Phase 01 P01]: from_pandas/to_pandas delegate to pandas' own __arrow_c_stream__ export and pyarrow's own Table.to_pandas(types_mapper=pandas.ArrowDtype), avoiding hand-rolled FFI and private pandas attributes
- [Phase 01 P02]: Genuine zero-copy numpy borrow implemented by hand (Buffer::from_custom_allocation + Py<PyArray1<T>> owner) rather than pyo3-arrow's from_numpy(), which was found by reading its source to copy via PrimitiveArray::from_iter_values even on its contiguous fast path
- [Phase 01 P02]: flint.FlintError/ZeroCopyRequiredError implemented via pyo3::create_exception! with Py-prefixed Rust identifiers to avoid colliding with the internal thiserror FlintError enum
- [Phase 01 P02]: to_pandas intentionally does not call plan_column per column (every Table column is already Arrow memory, so the decision is always ZeroCopyBorrow) -- documented as a deviation rather than adding a symbolic always-same-result call
- [Phase 01]: Rust allocation proof asserts info.bytes_total against an 80,000-byte fixture (threshold 1024 bytes) rather than the RESEARCH.md sketch's literal count_total == 0: arrow-buffer's Buffer::from_custom_allocation unconditionally makes a small constant Arc<Bytes> allocation, and wrapping as ArrayRef costs a second constant allocation -- neither copies the data buffer, but together they make count_total == 0 unreachable for any correct binding into arrow-rs's real API
- [Phase 01]: flint_core::from_numpy_buffer implemented in Plan 03 (not Plan 01), per 01-02-SUMMARY.md's explicit note that the stub was ready for Plan 03 to fill in -- pyo3-free, unsafe fn, no owner-lifetime tracking, exists solely as the D-06b allocation-counting proof's measured entry point
- [Phase 01 P04]: from_arrow's obj parameter is typed &Bound<PyAny> (not pyo3_arrow::PyTable) so the extraction call site is explicit and its errors can be remapped onto flint.FlintError -- binding as PyTable directly would run extraction during PyO3's own argument-binding step, before any remap is possible
- [Phase 01 P04]: Untrusted-capsule validation errors are remapped onto diagnostics::PyFlintError (the Python-visible flint.FlintError), not crate::error::FlintError (the Plan 01/02 internal thiserror enum, which maps to builtin PyValueError/PyTypeError and is never visible as flint.FlintError)
- [Phase 01 P04]: DuckDB Open Question 1 / Assumption A2 resolved empirically: pinned duckdb 1.5.4 consumes a flint Table natively via duckdb.sql("FROM <obj>").arrow().read_all(), no pyarrow intermediary needed -- documented fallback implemented but unused
- [Phase ?]: [Quick 260715-smf] Concatenate multi-batch columns via arrow::compute::concat rather than rejecting multi-chunk input outright (fixes CR-01 silent truncation), while keeping the single-batch fast path as a direct Arc clone with no concat call
- [Phase 01 verification]: DIAG-01/DIAG-02 multi-chunk diagnostics-honesty gap (strict mode / copy_report don't detect the CR-01 fix's concat copy for multi-chunk columns) accepted via recorded override rather than fixed immediately -- root cause (plan_column has no chunk-count visibility) is the same mechanism CONV-08 needs to solve anyway, so bundling the fix there avoids a throwaway patch. Accepted by John Columna 2026-07-15.
- [Phase 02 P01]: classify_dtype restructured from dtype.kind-first to isinstance-first dispatch -- the enabling mechanism every later dtype-family slice (string, categorical, datetime/tz, timedelta) extends; also fixed a Rule-1 bug where the masked-extension rejection path mapped to builtin PyTypeError instead of flint.FlintError.
- [Phase 02 P02]: object-dtype validation (D-11) implemented as a Flint-owned pre-conversion scan (validate_object_column_contents) rather than trusting pyarrow's permissive type inference, per RESEARCH Pitfall 2.
- [Phase 02 P03]: Categorical round-trip fidelity required two separate fixes: Field::new_dictionary + with_dict_is_ordered to stop from_pandas silently dropping the dictionary ordered flag, and a per-column-type-aware to_pandas types_mapper closure to stop dictionary columns reconstructing as ArrowDtype instead of real pd.Categorical.
- [Phase 02 P05]: DIAG-01/DIAG-02 resolved via Strategy B -- import_column_via_pandas_stream now returns the observed RecordBatch count, and from_pandas corrects the already-computed ColumnConversionRecord post-hoc when count > 1, rather than giving plan_column its own chunk-count-aware second decision path. diagnostics.rs required no change. strict=True now correctly rejects multi-chunk columns with no bypass flag.
- [Phase 02 gate tooling]: Post-merge/regression gates were sniffing to bare `cargo test`, which never rebuilds the installed PyO3 extension via `maturin develop` -- Python-visible regressions from merged Rust changes went undetected until a manual full pytest run after wave 5 (21 stale-build failures, all resolved by rebuilding, zero were real code defects). Fixed via explicit `workflow.build_command`/`workflow.test_command` in config.json so future waves/phases in this project catch this class of regression.
- [Phase ?]: [Phase 03 P01]: Wave-0 A6 gate PASSED empirically -- arrow-rs default embedded ARROW:schema metadata preserves DataType::Dictionary(dict_is_ordered) and exact tz strings through a bare Parquet round-trip; Plans 02-04 rely on this default, no explicit schema-hint mechanism needed
- [Phase ?]: [Phase 03 P01]: flint-core::parquet_io returns parquet::errors::ParquetError (not FlintError) since flint-core cannot depend on flint-python's pyo3-coupled error type; mapped to FlintError::Other at the from_parquet/to_parquet PyO3 boundary

### Pending Todos

None yet.

### Blockers/Concerns

- ~~Phase 2 (carried forward from Phase 1 verification override): CONV-08 DIAG-01/DIAG-02 multi-chunk diagnostics honesty gap~~ -- **Resolved** in Phase 2 Plan 05 (see 02-VERIFICATION.md and 02-05-SUMMARY.md).
- Phase 3 (from 02-REVIEW.md WR-01, demonstrated/reproducible): `build_field` in `crates/flint-python/src/pandas.rs` derives Arrow field nullability from the current batch's observed `null_count() > 0` rather than the source pandas dtype's declared nullability. A nullable `int64[pyarrow]` column with zero nulls round-trips as a `not null` Flint schema field, which breaks `pyarrow.concat_tables` against a genuinely-nullable sibling batch (`ArrowInvalid` schema mismatch). Not fixed as of Phase 2 close; worth fixing before/during Phase 3 given Parquet schema fidelity is exactly this class of concern.
- Phase 3 (from 02-REVIEW.md WR-02, structurally real but not reproduced under pinned config): the zero-copy numpy buffer borrow (`borrow_numpy_numeric_column`/`NumpyBufferOwner`) has no independent immutability guarantee — it relies entirely on pandas' Copy-on-Write to prevent post-borrow mutation from corrupting the Arrow buffer. Did not reproduce under pinned pandas 3.0.3 (CoW blocked all three tried mutation paths), but CLAUDE.md claims `pandas >= 2.2` support with no runtime floor pinned in pyproject.toml, and CoW is off by default pre-3.0 — a latent gap on nominally-supported configurations.
- Phase 3 (research-flagged): categorical/dictionary Parquet round-trip edge cases and tz-aware timestamp handling warrant verification against current pyarrow issues (#35259, #1688) at plan time.
- Phase 3 (research-flagged): confirm pandas ArrowDtype import-side support status (pandas 3.0.x) before finalizing pandas-interop reverse direction — may affect Phase 2 design already, verify at Phase 2 plan time too.
- Phase 4 (research-flagged): benchmarking methodology (criterion/pytest-benchmark/codspeed) and manylinux/glibc floor are MEDIUM-confidence, task-derived recommendations — validate current best practice at plan time.

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 260715-smf | Fix CR-01: from_pandas silently truncates multi-chunk Arrow-backed pandas columns to only the first chunk | 2026-07-15 | b5df2da | [260715-smf-fix-cr-01-from-pandas-silently-truncates](./quick/260715-smf-fix-cr-01-from-pandas-silently-truncates/) |

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| v2 Requirements | IO-01 (CSV read/write), IO-02 (JSON read/write) | Deferred to v2 | Project init |

## Session Continuity

Last session: 2026-07-23T12:54:06.413Z
Stopped at: Completed 03-01-PLAN.md
Resume file: None
