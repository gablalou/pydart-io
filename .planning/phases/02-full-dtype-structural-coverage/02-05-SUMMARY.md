---
phase: 02-full-dtype-structural-coverage
plan: 05
subsystem: conversion
tags: [rust, pyo3, pandas, arrow, diagnostics, strict-mode, multi-chunk, copy_report]

# Dependency graph
requires:
  - phase: 02-full-dtype-structural-coverage
    provides: "01-04 plans: classify_dtype full dtype matrix (numeric/bool/string/categorical/timestamp/duration), plan_column decision matrix, import_column_via_pandas_stream multi-chunk concat fallback (CR-01)"
  - phase: 01-core-zero-copy-round-trip-interop
    provides: "01-VERIFICATION.md's recorded DIAG-01/DIAG-02 override -- the diagnostics-honesty gap this plan closes"
provides:
  - "import_column_via_pandas_stream returns (ArrayRef, usize) -- the observed RecordBatch count alongside the array"
  - "from_pandas post-hoc ColumnConversionRecord correction: a column whose stream import observed >1 batches is corrected to zero_copy=false with a multi-chunk/concat reason, before check_strict/build_copy_report consume the records"
  - "strict=True now raises flint.ZeroCopyRequiredError for a multi-chunk Arrow-backed column, with no bypass flag (D-14 behavior change)"
  - "copy_report() now honestly reports a multi-chunk column as zero_copy=False with a chunk/concat reason (D-13)"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Post-hoc diagnostics correction (Strategy B, RESEARCH.md Pitfall 6): a function that only has the count it observed (import_column_via_pandas_stream) surfaces that count to its caller, which corrects the SAME already-computed decision record in place -- never a second, parallel decision matrix. This is the second application of this project's single-source-of-truth pattern (the first being plan_column/ColumnConversionRecord itself)."

key-files:
  created:
    - tests/python/test_multi_chunk_diagnostics.py
  modified:
    - crates/flint-python/src/pandas.rs

key-decisions:
  - "import_column_via_pandas_stream's empty-batch (0-row) branch returns count 0 (not 1) -- since the correction only fires for count > 1, this is behaviorally identical to returning 1 for that branch, but 0 is the honest observed count and avoids inventing a batch that was never seen."
  - "The correction is applied via records.last_mut() immediately after the array match arm that calls import_column_via_pandas_stream, reusing the existing per-column loop structure rather than restructuring from_pandas's control flow -- the record was already pushed earlier in the same loop iteration, so last_mut() unambiguously targets the current column's record."

requirements-completed: [CONV-08]

coverage:
  - id: D12
    description: "A multi-chunk Arrow-backed pandas column (pd.concat of two int64[pyarrow] frames -> 2-chunk ChunkedArray) still round-trips ALL rows through from_pandas -> to_pandas (the arrow::compute::concat fallback retained, an honest copy, not zero-copy) -- CR-01 not regressed"
    requirement: "CONV-08"
    verification:
      - kind: unit
        ref: "tests/python/test_multi_chunk_diagnostics.py#test_multi_chunk_column_still_round_trips_all_rows"
        status: pass
      - kind: unit
        ref: "tests/python/test_round_trip.py#test_from_pandas_preserves_all_rows_of_multi_chunk_arrow_backed_column"
        status: pass
    human_judgment: false
  - id: D13
    description: "plan_column/ColumnConversionRecord is now chunk-count-aware -- a multi-chunk column's record is corrected to zero_copy=false with a reason attributing the copy to multi-chunk concatenation, closing the DIAG-01/DIAG-02 override from 01-VERIFICATION.md"
    requirement: "CONV-08"
    verification:
      - kind: unit
        ref: "tests/python/test_multi_chunk_diagnostics.py#test_multi_chunk_column_reported_as_copy_in_copy_report"
        status: pass
    human_judgment: false
  - id: D14
    description: "from_pandas(df, strict=True) now RAISES flint.ZeroCopyRequiredError for a multi-chunk Arrow-backed column that previously succeeded silently -- no opt-in flag to bypass"
    requirement: "CONV-08"
    verification:
      - kind: unit
        ref: "tests/python/test_multi_chunk_diagnostics.py#test_strict_mode_now_rejects_multi_chunk_column"
        status: pass
    human_judgment: false
  - id: D12b
    description: "A single-chunk Arrow-backed column is unaffected: still reports zero_copy=true (copy_report) and still succeeds under strict=True -- the chunk-count correction only fires when observed batch count > 1"
    requirement: "CONV-08"
    verification:
      - kind: unit
        ref: "tests/python/test_multi_chunk_diagnostics.py#test_single_chunk_arrow_column_still_zero_copy"
        status: pass
      - kind: unit
        ref: "tests/python/test_multi_chunk_diagnostics.py#test_single_chunk_arrow_column_still_succeeds_under_strict"
        status: pass
    human_judgment: false
  - id: D-agree
    description: "copy_report() and strict mode agree on exactly which column is non-zero-copy for the multi-chunk case (single source of truth, T-01-05)"
    requirement: "CONV-08"
    verification:
      - kind: unit
        ref: "tests/python/test_multi_chunk_diagnostics.py#test_copy_report_and_strict_agree_for_multi_chunk"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-07-21
status: complete
---

# Phase 2 Plan 05: Multi-Chunk Diagnostics Honesty (CONV-08) Summary

**import_column_via_pandas_stream now surfaces its observed RecordBatch count; from_pandas uses it to correct a multi-chunk column's ColumnConversionRecord post-hoc, closing the DIAG-01/DIAG-02 diagnostics-honesty gap carried forward from 01-VERIFICATION.md.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-07-21T14:19:00Z (approx, first read)
- **Completed:** 2026-07-21T14:32:29Z
- **Tasks:** 2
- **Files modified:** 2 (1 created, 1 modified)

## Accomplishments

- `import_column_via_pandas_stream` (`crates/flint-python/src/pandas.rs`) changed its return type from `PyResult<ArrayRef>` to `PyResult<(ArrayRef, usize)>`: the empty-batch branch returns `(array, 0)`, the single-batch branch returns `(array, 1)`, and the multi-batch concat branch returns `(concatenated, batches.len())`. The internal concat/single-batch logic itself is unchanged (still correct per CR-01) -- only the observed count is now surfaced.
- `from_pandas`'s per-column loop destructures `(array, observed_batch_count)` at its single call site of `import_column_via_pandas_stream` and, when `observed_batch_count > 1`, corrects that column's already-pushed `ColumnConversionRecord` in place (`records.last_mut()`) to `zero_copy=false` with a reason naming the chunk count and attributing the copy to `arrow::compute::concat`. This happens strictly before `from_pandas` returns, so `check_strict`/`build_copy_report` (both in `diagnostics.rs`, unchanged) see the corrected, honest record.
- `strict=True` now raises `flint.ZeroCopyRequiredError` for a multi-chunk Arrow-backed column (D-14) -- the correct DIAG-01 contract behavior, with no bypass flag added. `copy_report()` now reports the same column as `zero_copy=False` with a chunk/concat reason (D-13). A single-chunk Arrow-backed column is unaffected (correction only fires for `observed_batch_count > 1`), confirmed by a dedicated test.
- `diagnostics.rs` required zero changes (verified via `git diff` against the phase's merge-base commit) -- both `check_strict` and `build_copy_report` already only read `record.zero_copy`/`record.reason`, so the correction step in `pandas.rs` alone is sufficient. `plan_column`'s pure-Rust signature is also unchanged -- the correction is a refinement of the single already-computed decision, not a second decision matrix (RESEARCH.md Pitfall 2).
- Audited `tests/python` for any existing assertion expecting a multi-chunk column to succeed under `strict=True`: none found. `test_round_trip.py`'s CR-01 fixture uses default `strict=False`, and `test_strict_mode.py` has no multi-chunk case at all -- confirmed by direct inspection and grep, not assumed. No existing test needed updating for the D-14 behavior change.

## Task Commits

Each task was committed atomically:

1. **Task 1: import_column_via_pandas_stream returns (ArrayRef, usize); from_pandas corrects the ColumnConversionRecord post-hoc (Strategy B)** - `134a4b7` (feat)
2. **Task 2: Audit existing tests for the D-14 behavior change, then add multi-chunk diagnostics tests** - `2c10610` (test)

_Note: this SUMMARY's own metadata commit is created separately per worktree execution rules (STATE.md/ROADMAP.md are NOT updated here -- the orchestrator owns those after all wave agents complete)._

## Files Created/Modified

- `crates/flint-python/src/pandas.rs` - `import_column_via_pandas_stream` return type changed to `PyResult<(ArrayRef, usize)>`; `from_pandas`'s array-selection match arm destructures the tuple and applies the post-hoc `ColumnConversionRecord` correction for `observed_batch_count > 1`; doc comments updated to describe the new return contract.
- `tests/python/test_multi_chunk_diagnostics.py` - new: 6 tests covering D-12 (round-trip preserved), D-13 (copy_report honesty), single-chunk-unaffected (both `copy_report` and `strict`), D-14 (strict now rejects), and copy_report/strict agreement for the multi-chunk case.

## Decisions Made

- The empty-batch (0-row) branch of `import_column_via_pandas_stream` returns count `0`, not `1`. Since the correction only fires for `count > 1`, this is behaviorally identical to returning `1`, but `0` is the honest count of batches actually observed (no batch was seen at all for a genuinely empty column) rather than a synthetic placeholder.
- The correction is applied via `records.last_mut()` immediately after the array-selection match arm, reusing the existing per-column loop structure (the record for the current column was already pushed earlier in the same loop iteration by `records.push(ColumnConversionRecord::from_plan(...))`), rather than restructuring `from_pandas`'s control flow to defer the push. This keeps the diff minimal and the correction visibly scoped to "the record we just computed for this exact column."

## Deviations from Plan

None - plan executed exactly as written. Task 1's implementation matches RESEARCH.md's recommended Strategy B precisely (no private pandas API queried, `plan_column`'s signature untouched, `diagnostics.rs` untouched). Task 2's audit confirmed the plan's own stated expectation ("no existing test should need updating") empirically rather than assuming it.

## Issues Encountered

One process note (not a plan deviation): the first verification run was executed against the wrong working directory (the main repo checkout at `/home/pc_gab_c/dev/test-project-sample` rather than this worktree at `.../.claude/worktrees/agent-a76c7470d9ec55ad5`), because the plan's `<verify>` command literally hardcodes that path. This produced a misleading stale-wheel test failure (18 failures reporting a pre-Phase-2 short rejection message) that had nothing to do with this plan's changes. Re-ran `uv run maturin develop && uv run pytest tests/python -q` with the default (worktree) working directory instead, which built and tested this worktree's actual code and passed cleanly (61/61). No source files were affected by this false alarm; flagged here only as an execution-environment note, not a code deviation.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- CONV-08 is now fully delivered: multi-chunk columns round-trip correctly (D-12), are honestly reported as copies (D-13), and are rejected under strict mode with no bypass (D-14). The DIAG-01/DIAG-02 override recorded in `01-VERIFICATION.md` is closed.
- This is the final plan of Phase 2 (wave 5, depends on 02-01 through 02-04). No further plans in this phase depend on this work; the phase's full dtype/structural coverage plus the diagnostics-honesty gap are both complete.
- Full workspace verification at completion: `cargo test --workspace` (10/10 Rust tests pass, unchanged) and `uv run pytest tests/python -q` (61/61 Python tests pass: 55 pre-existing + 6 new).

---
*Phase: 02-full-dtype-structural-coverage*
*Completed: 2026-07-21*

## Self-Check: PASSED

Confirmed `crates/flint-python/src/pandas.rs` and `tests/python/test_multi_chunk_diagnostics.py` present
on disk with the expected content. Confirmed both task commits (`134a4b7`, `2c10610`) present in
`git log --oneline --all`. Confirmed `diagnostics.rs` has zero diff against the phase's wave-4
merge-base commit (`95c8e03`). Confirmed full test suites green: `cargo test --workspace` and
`uv run pytest tests/python -q` (61 passed).
