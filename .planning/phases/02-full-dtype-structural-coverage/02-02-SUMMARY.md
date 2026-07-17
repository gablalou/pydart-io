---
phase: 02-full-dtype-structural-coverage
plan: 02
subsystem: conversion
tags: [rust, pyo3, pandas, arrow, pyarrow, classify_dtype, strings, object-dtype, validation]

# Dependency graph
requires:
  - phase: 02-full-dtype-structural-coverage
    plan: 01
    provides: "classify_dtype isinstance-first dispatch skeleton with explicit extension-point comments; FlintError::UnsupportedColumn mapped to flint.FlintError"
provides:
  - "ArrowKind::String variant + plan_column arms: (Arrow, String) -> ZeroCopyBorrow, (Numpy, String) -> RequiresCopy with reason"
  - "classify_dtype extended: ArrowDtype is_string/is_large_string -> ArrowKind::String; numpy kind 'O' -> ArrowKind::String"
  - "validate_object_column_contents: Flint-owned D-11 content validation pass for numpy object-dtype columns, run before any conversion is attempted"
  - "import_column_via_pandas_stream fix: genuinely empty (0-row) columns now construct an empty array from schema instead of erroring"
  - "CONV-04 proven end-to-end for both string backends with a complete D-11 rejection matrix (dict, int, both mixed-type orderings)"
affects: [02-03, 02-04, 02-05]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Flint-owned pre-conversion validation pass (validate_object_column_contents), modeled on borrow_numpy_numeric_column's (series, column_name) -> PyResult<T> signature/error style -- runs BEFORE import_column_via_pandas_stream so a non-str value is rejected before any conversion is attempted, never relying on pyarrow's own inference"
    - "Empty-stream handling: PyTable::into_inner() retains the imported SchemaRef even when batches is empty, allowing arrow::array::new_empty_array(schema.field(0).data_type()) to construct a genuine empty array instead of treating zero batches as an error"

key-files:
  created:
    - tests/python/test_object_string.py
  modified:
    - crates/flint-core/src/pandas_plan.rs
    - crates/flint-python/src/pandas.rs

key-decisions:
  - "ArrowDtype string sub-classification checks BOTH pa.types.is_string AND pa.types.is_large_string (Assumption A2 confirmed: pa.types.is_string(pa.large_string()) is False, so large_string[pyarrow] would be wrongly rejected without the second check)"
  - "[Rule 1 - Bug] Fixed import_column_via_pandas_stream to construct an empty array from the stream's schema when batches.is_empty(), instead of raising FlintError::Other('column stream produced no record batches') -- required by this plan's own must-have truth that empty/all-null object columns convert without error"
  - "Confirmed via empirical probe (not assumed): an empty or all-None object-dtype column's Arrow type resolves to null[pyarrow] (Arrow's null type), not string -- this resolves the plan's FLAGGED ASSUMPTION for the empty/all-null edge cases; tests assert the no-error/all-null contract, not a specific Arrow type"
  - "Genuine ArrowDtype string columns must be constructed via pandas.ArrowDtype(pyarrow.string()), NOT the dtype='string[pyarrow]' string alias -- that alias resolves to pandas.StringDtype(storage='pyarrow'), a masked ExtensionDtype rejected the same honest way as masked Int64/boolean (Plan 01's D-08 path), confirmed empirically before writing tests"

requirements-completed: [CONV-04]

coverage:
  - id: D1
    description: "An ArrowDtype-backed string column (pandas.ArrowDtype(pyarrow.string())) round-trips from_pandas -> to_pandas with correct values and null handling, and copy_report marks it zero_copy=true (D-10)"
    requirement: "CONV-04"
    verification:
      - kind: unit
        ref: "tests/python/test_object_string.py#test_arrow_dtype_string_round_trips_zero_copy"
        status: pass
    human_judgment: false
  - id: D2
    description: "A legacy numpy object-dtype column of Python str values (with a None) round-trips via an honest copy, reported zero_copy=false with a reason naming the object dtype's lack of Arrow-compatible physical layout (D-10)"
    requirement: "CONV-04"
    verification:
      - kind: unit
        ref: "tests/python/test_object_string.py#test_numpy_object_string_round_trips_via_copy"
        status: pass
    human_judgment: false
  - id: D3
    description: "Object-dtype columns containing any non-str, non-null value (int, dict, or either ordering of a genuinely mixed column) are rejected with a Flint-owned flint.FlintError naming the column and offending value's type -- never relying on pyarrow's own permissive inference (D-11 / RESEARCH Pitfall 2)"
    requirement: "CONV-04"
    verification:
      - kind: unit
        ref: "tests/python/test_object_string.py#test_object_column_of_ints_rejected"
        status: pass
      - kind: unit
        ref: "tests/python/test_object_string.py#test_object_column_of_dicts_rejected"
        status: pass
      - kind: unit
        ref: "tests/python/test_object_string.py#test_object_column_mixed_str_then_int_rejected"
        status: pass
      - kind: unit
        ref: "tests/python/test_object_string.py#test_object_column_mixed_int_then_str_rejected"
        status: pass
    human_judgment: false
  - id: D4
    description: "FLAGGED ASSUMPTION resolved: an empty or all-None object-dtype column converts without error (confirmed: resulting Arrow type is null[pyarrow], not string, for these two edge cases)"
    requirement: "CONV-04"
    verification:
      - kind: unit
        ref: "tests/python/test_object_string.py#test_empty_object_column_converts_without_error"
        status: pass
      - kind: unit
        ref: "tests/python/test_object_string.py#test_all_none_object_column_converts_without_error"
        status: pass
    human_judgment: false
  - id: D5
    description: "ArrowKind::String + plan_column matrix arms (Arrow->ZeroCopyBorrow, Numpy->RequiresCopy) added with Rust unit tests, no regression to cargo test -p flint-core"
    verification:
      - kind: unit
        ref: "crates/flint-core/src/pandas_plan.rs#plan_column_arrow_string_is_zero_copy_borrow"
        status: pass
      - kind: unit
        ref: "crates/flint-core/src/pandas_plan.rs#plan_column_numpy_string_requires_copy"
        status: pass
      - kind: unit
        ref: "cargo test -p flint-core (12/12 total, no regression)"
        status: pass
    human_judgment: false

duration: 35min
completed: 2026-07-17
status: complete
---

# Phase 2 Plan 02: Object/String Dtype Coverage (CONV-04) Summary

**Delivered CONV-04's string story end-to-end: ArrowKind::String + plan_column matrix arms, classify_dtype string branches for both string[pyarrow] and legacy numpy object columns, and a Flint-owned pre-conversion content-validation pass that closes pyarrow's silent-int64/silent-struct inference gaps.**

## Performance

- **Duration:** ~35 min
- **Started:** 2026-07-16T20:54:38+08:00
- **Completed:** 2026-07-17T12:58:52+08:00 (commit timestamp; active work time is ~35 min per task cadence)
- **Tasks:** 3
- **Files modified:** 3 (1 created, 2 modified)

## Accomplishments

- `ArrowKind::String` added to `crates/flint-core/src/pandas_plan.rs` with two new `plan_column` match arms: `(Arrow, String) -> ZeroCopyBorrow` (already Arrow memory) and `(Numpy, String) -> RequiresCopy` with a full-sentence reason (boxed Python `str` pointers, no contiguous Arrow-compatible UTF-8 buffer). Module doc matrix table and two new unit tests (`plan_column_arrow_string_is_zero_copy_borrow`, `plan_column_numpy_string_requires_copy`) added.
- `classify_dtype` (`crates/flint-python/src/pandas.rs`) extended: the `ArrowDtype` branch now checks `pa.types.is_string` OR `is_large_string` (Assumption A2 confirmed empirically -- `is_string(pa.large_string())` is `False`) to map to `ArrowKind::String`; the plain-numpy `dtype.kind` branch now maps `"O"` to `(Numpy, ArrowKind::String)` instead of rejecting it.
- New `validate_object_column_contents` function: a Flint-owned D-11 content-validation pass that iterates a numpy object column's values in Python, skips `None`/`NaN`, and rejects the first non-`str` value with `FlintError::UnsupportedColumn` naming the column, dtype `"object"`, the offending `type(v).__name__`, and the row index. Called from `from_pandas`'s per-column loop for exactly the `(Numpy, String)` case, before `import_column_via_pandas_stream` is ever invoked -- proven empirically to close all four gaps RESEARCH.md's Pitfall 2 identified: silent dict->struct conversion, silent int->int64 conversion, and both orderings of a genuinely mixed-type column (which pyarrow's own inference raises as two *different*, order-dependent exception types for).
- `tests/python/test_object_string.py` created with 8 tests covering both string backends' round-trip + `copy_report` behavior (D-10) and the full D-11 rejection matrix (dict, int, both mixed orderings), plus the flagged empty/all-null edge cases.
- **Auto-fixed bug** (see Deviations): `import_column_via_pandas_stream` previously errored on any genuinely empty (0-row) column routed through the generic fallback -- fixed to construct an empty array from the stream's schema instead, which was required to satisfy this plan's own must-have truth for the empty/all-null object-column edge cases.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add ArrowKind::String + plan_column arms + Rust unit tests** - `0cecf02` (feat)
2. **Task 2: classify_dtype string branches + Flint-owned D-11 object-content validation pass** - `872fd45` (feat)
3. **[Rule 1 - Bug] Fix empty-stream columns constructing an empty array instead of erroring** - `c0b1ef0` (fix)
4. **Task 3: Python tests for CONV-04 -- both string paths + D-11 rejection matrix** - `d06dfa3` (test)

_Note: this SUMMARY's own metadata commit is created separately per worktree execution rules (STATE.md/ROADMAP.md are NOT updated here -- the orchestrator owns those after all wave agents complete)._

## Files Created/Modified

- `crates/flint-core/src/pandas_plan.rs` - `ArrowKind::String` variant + two `plan_column` match arms + two unit tests + module doc matrix table update
- `crates/flint-python/src/pandas.rs` - `classify_dtype` string branches (ArrowDtype is_string/is_large_string; numpy kind `"O"`); new `validate_object_column_contents` function; `from_pandas` per-column loop calls it for exactly `(Numpy, String)` before conversion; `import_column_via_pandas_stream` fixed to handle empty streams (deviation, see below)
- `tests/python/test_object_string.py` - new: CONV-04 tests (8 tests: ArrowDtype string round-trip, numpy object string round-trip, int/dict/mixed-both-orderings rejection, empty/all-null edge cases)

## Decisions Made

- ArrowDtype string sub-classification checks both `pa.types.is_string` and `pa.types.is_large_string` (Assumption A2 confirmed empirically), so `large_string[pyarrow]` is accepted alongside `string[pyarrow]`.
- Genuine `ArrowDtype` string columns must be constructed via `pandas.ArrowDtype(pyarrow.string())`, not the `dtype="string[pyarrow]"` string alias -- confirmed empirically that the alias resolves to `pandas.StringDtype(storage="pyarrow")` (a masked `ExtensionDtype`), which is correctly rejected by Plan 01's existing D-08 masked-extension rejection path, not accepted as Arrow-backed. Tests construct the genuine `ArrowDtype` explicitly and document this distinction.
- Confirmed via empirical probe that an empty or all-None object-dtype column's Arrow type resolves to `null[pyarrow]` (Arrow's null type), not `string` -- this resolves the plan's FLAGGED ASSUMPTION for the empty/all-null edge cases specifically (Assumption A2's broader encoding question, e.g. UTF-8 byte-for-byte equality for non-empty string values, is unaffected and remains as documented in the plan).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `import_column_via_pandas_stream` errored on genuinely empty (0-row) columns**
- **Found during:** Task 3, while empirically probing the plan's flagged empty/all-null object-column assumption before writing the test file (per plan's own instruction to surface, not silently drop, this assumption)
- **Issue:** A column's `__arrow_c_stream__` export yields ZERO record batches for a genuinely empty (0-row) DataFrame column (confirmed via direct `pa.RecordBatchReader._import_from_c_capsule` probe against pandas 3.0.3/pyarrow 25.0.0), even though the stream's schema is still available. `import_column_via_pandas_stream` previously treated `batches.is_empty()` unconditionally as an error (`FlintError::Other("column stream produced no record batches")`). This is NOT specific to object/string columns -- an empty `int64[pyarrow]` column hits the identical error, confirming this is a pre-existing, generic gap in the fallback path this plan's must-have truth for empty object columns directly exposed.
- **Fix:** Changed `import_column_via_pandas_stream` to use `PyTable::into_inner()` (retaining the imported `SchemaRef` even when `batches` is empty) and construct a genuinely empty array via `arrow::array::new_empty_array(schema.field(0).data_type())` when `batches.is_empty()`, instead of erroring. The `batches.len() == 1` fast path and the multi-batch `arrow::compute::concat` fallback are unchanged.
- **Files modified:** `crates/flint-python/src/pandas.rs`
- **Verification:** Empirically confirmed before (raw `ValueError: column stream produced no record batches` for both an empty object column AND an empty `int64[pyarrow]` column) and after (both convert without error; empty object column's Arrow type is `null[pyarrow]`, 0 rows; empty `int64[pyarrow]` column round-trips as `int64[pyarrow]`, 0 rows) the fix. Full existing pytest suite (34/34) and `cargo test -p flint-core` (12/12) both green after the fix; final suite with new tests added is 42/42 green.
- **Committed in:** `c0b1ef0`

---

**Total deviations:** 1 auto-fixed (Rule 1 - bug)
**Impact on plan:** Necessary to satisfy this plan's own must-have truth (the empty/all-null object-dtype FLAGGED ASSUMPTION explicitly requires "converts... without error"). Confined to `import_column_via_pandas_stream`'s zero-batches branch only -- its return signature (`PyResult<ArrayRef>`), single-batch fast path, and multi-batch concat fallback are all unchanged, so this does not touch the D-13/Plan-05-owned return-signature change PATTERNS.md flags as out of scope for this plan.

## Issues Encountered

None beyond the deviation documented above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `ArrowKind::String` and its two `plan_column` arms are in place; the module doc matrix table documents both rows for future readers.
- `classify_dtype`'s extension-point comments updated to reflect Plan 02's additions, so Plan 03 (categorical) and Plan 04 (timestamp/duration) can continue inserting their own isinstance checks at the documented locations without re-deriving dispatch order.
- `validate_object_column_contents` establishes the pattern for any future Flint-owned pre-conversion content validation pass (small, single-purpose, modeled on `borrow_numpy_numeric_column`'s signature/error style) -- reusable if a later plan needs similar explicit validation for another permissive-inference dtype.
- The `import_column_via_pandas_stream` empty-stream fix benefits ALL columns routed through the generic fallback (not just string), closing a latent gap Plan 03 (categorical) and Plan 04 (timestamp/duration) would otherwise have independently rediscovered for their own empty-column cases.
- Carried-forward blocker (unchanged by this plan): CONV-08 (multi-chunk `Table<->pandas` diagnostics-honesty gap, DIAG-01/DIAG-02) remains deferred to Plan 05 per Phase 1's recorded override and this phase's D-13/Pitfall 6 Strategy B assignment.

---
*Phase: 02-full-dtype-structural-coverage*
*Completed: 2026-07-17*

## Self-Check: PASSED

All created/modified files confirmed present on disk (pandas_plan.rs, pandas.rs,
test_object_string.py, this SUMMARY.md). All 5 commits confirmed present in
`git log --oneline --all` (0cecf02, 872fd45, c0b1ef0, d06dfa3, 5f3bb37).
