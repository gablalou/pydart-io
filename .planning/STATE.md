---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
current_phase: 01
current_phase_name: core-zero-copy-round-trip-interop
status: verifying
stopped_at: Completed quick task 260715-smf (fix CR-01 from_pandas silent truncation)
last_updated: "2026-07-15T13:16:50.731Z"
last_activity: 2026-07-14
last_activity_desc: Completed 01-02-PLAN.md
progress:
  total_phases: 1
  completed_phases: 0
  total_plans: 5
  completed_plans: 4
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-07-13)

**Core value:** Converting a pandas DataFrame to/from an Arrow Table should be zero-copy (or as close to it as physically possible) and measurably faster than pyarrow — this must work and must be provably faster, or the project has no reason to exist.
**Current focus:** Phase 01 — core-zero-copy-round-trip-interop

## Current Position

Phase: 01 (core-zero-copy-round-trip-interop) — EXECUTING
Plan: 4 of 4
Status: Phase complete — ready for verification
Last activity: 2026-07-14 — Completed 01-02-PLAN.md

Progress: [█████░░░░░] 50%

## Performance Metrics

**Velocity:**

- Total plans completed: 0
- Average duration: - min
- Total execution time: 0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

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

### Pending Todos

None yet.

### Blockers/Concerns

- Phase 2 (carried forward from Phase 1 verification override): CONV-08 (multi-chunk Table<->pandas) must also make `plan_column`/`ColumnConversionRecord` chunk-count-aware so `strict=True` and `copy_report()` honestly reflect the concat copy for multi-chunk columns (DIAG-01/DIAG-02) -- this was deferred here rather than fixed in Phase 1, see 01-VERIFICATION.md overrides.
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

Last session: 2026-07-15T13:16:50.725Z
Stopped at: Completed quick task 260715-smf (fix CR-01 from_pandas silent truncation)
Resume file: None
