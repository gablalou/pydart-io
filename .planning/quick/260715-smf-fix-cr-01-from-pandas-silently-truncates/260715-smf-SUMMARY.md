---
phase: quick
plan: 260715-smf
subsystem: interop
tags: [rust, pyo3, arrow, pandas, arrow-compute-concat, bugfix]

requires:
  - phase: 01-core-zero-copy-round-trip-interop
    provides: from_pandas/to_pandas conversion pipeline (plan_column, import_column_via_pandas_stream)
provides:
  - Multi-chunk Arrow-backed pandas columns round-trip through from_pandas/to_pandas without
    silent row truncation (CR-01 fix)
  - Regression test locking in the fix for pd.concat-produced 2-chunk ChunkedArray columns
affects: [phase-02-dtype-coverage, phase-01-verification]

tech-stack:
  added: []
  patterns:
    - "import_column_via_pandas_stream accounts for every batch in a column's __arrow_c_stream__
       export: single-batch returns an Arc<dyn Array> clone (zero-copy), multi-batch concatenates
       via arrow::compute::concat (honest copy)"

key-files:
  created: []
  modified:
    - crates/flint-python/src/pandas.rs
    - tests/python/test_round_trip.py

key-decisions:
  - "Concatenate multi-batch columns via arrow::compute::concat rather than rejecting multi-chunk
     input outright — pd.concat of Arrow-backed frames is an ordinary, in-scope construction, and
     silently rejecting it would trade one correctness bug for a usability regression"
  - "Kept the single-batch fast path as a direct Arc clone (no concat call at all) rather than
     always routing through concat, preserving the certified zero-copy guarantee for the ordinary
     single-chunk case"

patterns-established:
  - "Batch-accounting helpers must explicitly branch on 0 / 1 / N batches rather than assuming
     .first() is representative of the whole stream"

requirements-completed: [CONV-01, CONV-02]

coverage:
  - id: D1
    description: "import_column_via_pandas_stream concatenates all RecordBatches for multi-chunk
      columns while preserving the single-chunk zero-copy Arc-clone fast path"
    requirement: "CONV-01"
    verification:
      - kind: unit
        ref: "cargo test --workspace (flint_core::pandas_plan unit tests + zero_copy_alloc.rs allocation proofs)"
        status: pass
      - kind: integration
        ref: "tests/python/test_round_trip.py#test_from_pandas_preserves_all_rows_of_multi_chunk_arrow_backed_column"
        status: pass
    human_judgment: false
  - id: D2
    description: "Existing single-chunk zero-copy and numpy-borrow round-trip behavior is unaffected
      by the fix (no regression)"
    requirement: "CONV-02"
    verification:
      - kind: integration
        ref: "uv run pytest tests/python/ -q (29 tests passed)"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-07-15
status: complete
---

# Quick Task 260715-smf: Fix CR-01 (from_pandas silently truncates multi-chunk columns) Summary

**`import_column_via_pandas_stream` now concatenates every RecordBatch in a column's Arrow C
stream via `arrow::compute::concat`, fixing silent multi-row-loss on `pd.concat`-produced
Arrow-backed pandas columns while preserving the certified single-chunk zero-copy Arc-clone path.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-07-15T12:47:00Z
- **Completed:** 2026-07-15T12:59:00Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Fixed CR-01: multi-chunk Arrow-backed pandas columns (e.g. produced by `pd.concat`) no longer
  silently truncate to just the first RecordBatch's rows.
- Preserved the single-chunk zero-copy fast path exactly as before (`batches[0].column(0).clone()`,
  no concat call, no allocation) — verified unaffected by the existing Rust allocation-proof tests.
- Added a regression test that builds the exact 6-row/2-chunk `pd.concat` scenario documented in
  01-VERIFICATION.md and asserts full row count + values survive the round trip.

## Task Commits

Each task was committed atomically:

1. **Task 1: Concatenate all batches in import_column_via_pandas_stream, preserving the
   single-chunk zero-copy fast path** - `7d0bc52` (fix)
2. **Task 2: Add a multi-chunk round-trip regression test reproducing CR-01** - `b5df2da` (test)

_Note: no plan-metadata commit yet — the orchestrator handles the docs commit in a later step._

## Files Created/Modified
- `crates/flint-python/src/pandas.rs` - `import_column_via_pandas_stream` now branches on
  `batches.len()`: empty -> existing error, 1 -> direct Arc clone (zero-copy), >=2 -> collects each
  batch's column-0 array and concatenates via `arrow::compute::concat`, mapping errors through
  `FlintError::from`. Doc comment updated to describe the multi-chunk concat behavior.
- `tests/python/test_round_trip.py` - Added
  `test_from_pandas_preserves_all_rows_of_multi_chunk_arrow_backed_column`, which builds two 3-row
  `int64[pyarrow]` frames, `pd.concat`s them into a 6-row/2-chunk DataFrame, and asserts the
  `from_pandas().to_pandas()` round trip preserves `len(result) == 6` and
  `result["a"].tolist() == [1, 2, 3, 4, 5, 6]`.

## Decisions Made
- Concatenate rather than reject multi-chunk input: `pd.concat` of Arrow-backed frames is an
  ordinary, in-scope pandas construction (not an edge case to reject), so the fix must handle it
  correctly rather than trading silent truncation for a hard error.
- Kept the `batches.len() == 1` branch as a direct `Arc` clone, never routed through `concat`, so
  the already-certified single-chunk zero-copy allocation proofs in the Rust test suite remain
  valid without modification.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None. The fix was a straightforward 3-way branch (empty / single-batch / multi-batch) exactly as
specified in the plan's Task 1 action, and the maturin rebuild + pytest cycle in Task 2 worked on
the first attempt.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- CR-01 is resolved and locked in by a regression test; the "never silent copy/loss" invariant
  (DIAG-01/DIAG-02) referenced in 01-REVIEW.md/01-VERIFICATION.md no longer has this exception.
- No blockers introduced for Phase 2 (dtype coverage broadening) — the fix is scoped entirely to
  `import_column_via_pandas_stream`'s batch handling and does not touch `plan_column`,
  `classify_dtype`, or the `ColumnConversionRecord`/diagnostics wiring.
- The other findings from 01-REVIEW.md (WR-01..WR-04, IN-01, IN-02) remain out of scope and
  unaddressed by this quick task, as specified in the plan.

---
*Phase: quick*
*Completed: 2026-07-15*

## Self-Check: PASSED

- FOUND: crates/flint-python/src/pandas.rs
- FOUND: tests/python/test_round_trip.py
- FOUND commit: 7d0bc52
- FOUND commit: b5df2da
