---
phase: 02-full-dtype-structural-coverage
plan: 01
subsystem: conversion
tags: [rust, pyo3, pandas, arrow, pyarrow, classify_dtype, nulls, error-handling]

# Dependency graph
requires:
  - phase: 01-numeric-bridge-skeleton
    provides: "classify_dtype (kind-first dispatch), plan_column decision matrix, FlintError/flint.FlintError exception plumbing, import_column_via_pandas_stream multi-chunk concat fallback"
provides:
  - "classify_dtype restructured to isinstance-first dispatch (ArrowDtype -> pyarrow.types sub-classification; ExtensionDtype -> honest reject; else numpy dtype.kind), with extension-point comments for Plans 02-04"
  - "FlintError::UnsupportedColumn now maps to flint.FlintError (PyFlintError), not builtin PyTypeError -- required for D-08's honest-rejection truth"
  - "Assumption A1 confirmed via direct Rust unit test: arrow::compute::concat handles DictionaryArray, tz-aware TimestampArray, and DurationArray generically"
  - "CONV-03 nulls end-to-end: ArrowDtype nullable round-trip (D-07), masked-extension honest rejection (D-08), numpy NaN passthrough (D-09)"
affects: [02-02, 02-03, 02-04, 02-05]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "isinstance-first dtype classification: check isinstance(dtype, pandas.ArrowDtype) and isinstance(dtype, pandas.api.extensions.ExtensionDtype) before ever consulting dtype.kind, since kind alone cannot distinguish ArrowDtype/Categorical/masked-extension dtypes that share the same kind letter"
    - "Extension-point comments mark exactly where Plans 02-04 must insert their sub-kind/isinstance checks, preserving dispatch-order correctness across the phase"

key-files:
  created:
    - tests/rust/concat_generic_arrays.rs
    - tests/python/test_nulls.py
  modified:
    - crates/flint-python/src/pandas.rs
    - crates/flint-python/src/error.rs
    - crates/flint-core/Cargo.toml

key-decisions:
  - "[Rule 1 - Bug] FlintError::UnsupportedColumn's PyErr mapping changed from PyTypeError to PyFlintError (flint.FlintError) -- the plan's own D-08 must-have and Task 3 acceptance criteria require the masked-extension rejection to be catchable as flint.FlintError, which a builtin TypeError is not. Verified empirically before and after the change; no new FlintError variant added, same enum/call sites, only which Python exception class UnsupportedColumn maps to changed."
  - "ArrowDtype sub-classification now reads pyarrow.types.is_integer/is_floating/is_boolean on dtype.pyarrow_dtype rather than dtype.kind, per RESEARCH.md's isinstance-first pattern -- required so later plans (string/timestamp/duration ArrowDtype sub-kinds) can extend the same branch without re-deriving classification from kind."

requirements-completed: [CONV-03]

coverage:
  - id: D1
    description: "ArrowDtype-backed nullable numeric column (int64[pyarrow]/float64[pyarrow]) round-trips from_pandas -> to_pandas with null positions preserved (D-07)"
    requirement: "CONV-03"
    verification:
      - kind: unit
        ref: "tests/python/test_nulls.py#test_nullable_arrow_dtype_int_round_trips_with_nulls_preserved"
        status: pass
      - kind: unit
        ref: "tests/python/test_nulls.py#test_nullable_arrow_dtype_float_round_trips_with_nulls_preserved"
        status: pass
    human_judgment: false
  - id: D2
    description: "Masked pandas nullable extension columns (Int64/boolean) are rejected with an honest flint.FlintError naming the column and dtype type name, not a raw AttributeError (D-08 / Pitfall 1)"
    requirement: "CONV-03"
    verification:
      - kind: unit
        ref: "tests/python/test_nulls.py#test_masked_int64_extension_dtype_rejected_with_flint_error"
        status: pass
      - kind: unit
        ref: "tests/python/test_nulls.py#test_masked_boolean_extension_dtype_rejected_with_flint_error"
        status: pass
    human_judgment: false
  - id: D3
    description: "Plain numpy float64 NaN columns keep going through the unchanged zero-copy numeric path -- NaN survives as a literal float, no null bitmap introduced, copy_report zero_copy=True (D-09)"
    requirement: "CONV-03"
    verification:
      - kind: unit
        ref: "tests/python/test_nulls.py#test_numpy_float64_nan_is_not_treated_as_null"
        status: pass
    human_judgment: false
  - id: D4
    description: "Assumption A1: arrow::compute::concat succeeds (no panic, returns Ok) for multiple DictionaryArray, tz-aware TimestampArray, and DurationArray inputs, de-risking Plans 03-04's multi-chunk concat fallback"
    verification:
      - kind: unit
        ref: "tests/rust/concat_generic_arrays.rs#concat_succeeds_on_dictionary_arrays"
        status: pass
      - kind: unit
        ref: "tests/rust/concat_generic_arrays.rs#concat_succeeds_on_timestamp_arrays_with_timezone"
        status: pass
      - kind: unit
        ref: "tests/rust/concat_generic_arrays.rs#concat_succeeds_on_duration_arrays"
        status: pass
    human_judgment: false
  - id: D5
    description: "classify_dtype restructured to isinstance-first dispatch order (ArrowDtype -> ExtensionDtype-reject -> numpy dtype.kind), with no Phase 1 regression across the full existing pytest suite"
    requirement: "CONV-03"
    verification:
      - kind: unit
        ref: "tests/python (full suite, 34 tests) -- pytest tests/python -q"
        status: pass
    human_judgment: false

duration: 11min
completed: 2026-07-16
status: complete
---

# Phase 2 Plan 01: Nulls & isinstance-first classify_dtype Summary

**Restructured classify_dtype to isinstance-first dispatch, fixed the masked-extension AttributeError crash into an honest flint.FlintError, and confirmed nullable ArrowDtype round-trips and arrow::compute::concat's generic multi-type support.**

## Performance

- **Duration:** 11 min
- **Started:** 2026-07-16T11:37:12Z
- **Completed:** 2026-07-16T11:48:05Z
- **Tasks:** 3
- **Files modified:** 5 (2 created, 3 modified)

## Accomplishments

- `classify_dtype` (crates/flint-python/src/pandas.rs) now dispatches isinstance-first: `pandas.ArrowDtype` is sub-classified via `pyarrow.types` predicates on `dtype.pyarrow_dtype` (never `dtype.kind`), any other `pandas.api.extensions.ExtensionDtype` is rejected honestly before the numpy-only `.values.flags` access can ever be reached, and only plain numpy dtypes fall through to the original `dtype.kind` match. Extension-point comments mark exactly where Plans 02 (categorical/string), 03/04 (timestamp/duration/tz) must insert their own isinstance checks.
- Fixed the D-08/Pitfall 1 defect empirically confirmed before this plan: `flint.Table.from_pandas` on a masked `Int64`/`boolean` column previously crashed with a raw `AttributeError: 'IntegerArray' object has no attribute 'flags'`. It now raises a catchable `flint.FlintError` naming the column and the dtype's concrete type name (e.g. `Int64Dtype`).
- Nullable `ArrowDtype` numeric columns (`int64[pyarrow]`/`float64[pyarrow]` with real `pd.NA` nulls) confirmed to round-trip through `from_pandas`/`to_pandas` with null positions preserved -- this already worked mechanically via Phase 1's `__arrow_c_stream__` import; this slice proves it with tests.
- Plain numpy `float64` NaN columns confirmed unchanged: NaN survives as a literal float (not converted to an Arrow null), and `copy_report()` still reports `zero_copy=True` for that column.
- Assumption A1 de-risked via a direct, pandas-free Rust unit test: `arrow::compute::concat` succeeds on multiple `DictionaryArray`, tz-aware `TimestampNanosecondArray`, and `DurationNanosecondArray` inputs -- Plans 03-04 can rely on the existing multi-chunk concat fallback with no type-specific handling.

## Task Commits

Each task was committed atomically:

1. **Task 1: Restructure classify_dtype to isinstance-first dispatch** - `170a0f2` (feat)
2. **Task 2: Assumption A1 probe -- arrow::compute::concat on Dictionary/Timestamp(tz)/Duration** - `4a1257c` (test)
3. **Task 3: Python tests for CONV-03** - `a6022cd` (test)

_Note: this SUMMARY's own metadata commit is created separately per worktree execution rules (STATE.md/ROADMAP.md are NOT updated here -- the orchestrator owns those after all wave agents complete)._

## Files Created/Modified

- `crates/flint-python/src/pandas.rs` - `classify_dtype` restructured to isinstance-first dispatch (ArrowDtype -> pyarrow.types sub-classify; ExtensionDtype -> honest reject; else numpy `.kind`); `from_pandas` call site updated to pass `py` and the new `extension_dtype_type` argument
- `crates/flint-python/src/error.rs` - `FlintError::UnsupportedColumn`'s `PyErr` mapping changed from `PyTypeError` to `PyFlintError` (deviation, see below)
- `crates/flint-core/Cargo.toml` - new `[[test]]` entry registering `concat_generic_arrays`
- `tests/rust/concat_generic_arrays.rs` - new: Assumption A1 probe (3 tests: Dictionary, tz-aware Timestamp, Duration)
- `tests/python/test_nulls.py` - new: CONV-03 tests (5 tests: nullable int round-trip, nullable float round-trip, masked Int64 rejection, masked boolean rejection, numpy NaN passthrough)

## Decisions Made

- **[Rule 1 - Bug] Changed `FlintError::UnsupportedColumn`'s exception mapping from `PyTypeError` to `PyFlintError`.** Empirically confirmed (before making any change) that the existing `impl From<FlintError> for PyErr` mapped `UnsupportedColumn` to the builtin `PyTypeError`, which is NOT in `flint.FlintError`'s exception hierarchy (`PyFlintError` extends `PyException` directly). The plan's own D-08 must-have truth and Task 3's acceptance criteria explicitly require `flint.Table.from_pandas` to raise `flint.FlintError` for the masked-extension rejection -- a plain `TypeError` would not be caught by `pytest.raises(flint.FlintError)`. This is the only change that satisfies both Task 1's "reuse `FlintError::UnsupportedColumn`, no new variant" instruction and Task 3's `flint.FlintError` requirement simultaneously: same enum, same call sites, only the target exception class for that one match arm changed. Verified via a full pre/post empirical check (`python -c` reproduction) and the full pytest suite (34/34 pass, no regression -- the only existing consumer of `UnsupportedColumn`, `test_export_smoke.py`, catches generic `Exception` so is unaffected).
- ArrowDtype sub-classification reads `pyarrow.types.is_integer`/`is_floating`/`is_boolean` on `dtype.pyarrow_dtype`, matching RESEARCH.md's isinstance-first pattern exactly, rather than continuing to infer Arrow sub-kind from `dtype.kind`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `FlintError::UnsupportedColumn` PyErr mapping changed from `PyTypeError` to `PyFlintError`**
- **Found during:** Task 1 (classify_dtype restructure), confirmed necessary while implementing Task 3's tests
- **Issue:** PATTERNS.md documented the current `PyTypeError::new_err` mapping as the pattern to "reuse verbatim," but the plan's own D-08 must-have truth and Task 3's acceptance criteria require the masked-extension rejection to be catchable as `flint.FlintError` -- which `PyTypeError` is not (confirmed empirically: `isinstance(TypeError_instance, flint.FlintError)` is `False`). This is a genuine conflict between a stale planning-reference doc and the plan's binding, test-verified acceptance criteria.
- **Fix:** Changed the single `UnsupportedColumn` match arm in `crates/flint-python/src/error.rs`'s `impl From<FlintError> for PyErr` from `PyTypeError::new_err(...)` to `PyFlintError::new_err(...)`. No new `FlintError` variant added; `Arrow`/`Other`/`NotImplemented` arms untouched.
- **Files modified:** `crates/flint-python/src/error.rs`
- **Verification:** Empirically confirmed before (raw `AttributeError` for masked Int64, `TypeError` for unsupported object column, `isinstance(exc, flint.FlintError) == False`) and after (`flint.FlintError`/`_flint.PyFlintError`, `isinstance(exc, flint.FlintError) == True`) the change via direct `python -c` reproduction; full pytest suite (34/34) and full `cargo test -p flint-core` (10/10) both green.
- **Committed in:** `170a0f2` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - bug)
**Impact on plan:** Necessary to satisfy the plan's own must-have truth (D-08) and Task 3's stated acceptance criteria. No scope creep -- confined to a single match arm in a file already read (not modified) per Task 1's `read_first` list.

## Issues Encountered

None beyond the deviation documented above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `classify_dtype`'s isinstance-first dispatch skeleton is in place with explicit extension-point comments -- Plans 02 (categorical), 03/04 (string/timestamp/duration ArrowDtype sub-kinds), and any DatetimeTZDtype handling can insert their isinstance checks in the exact documented locations without re-deriving dispatch order.
- Assumption A1 confirmed: `arrow::compute::concat` needs no type-specific handling for Dictionary/Timestamp(tz)/Duration in the multi-chunk fallback Plans 03-04 will rely on.
- `flint.FlintError` is now the honest, catchable exception for ALL `UnsupportedColumn` rejections (not just this plan's masked-extension case) -- later plans' D-15 (non-ns temporal) and D-11 (object-content validation) rejections will automatically inherit this correct behavior when they reuse `FlintError::UnsupportedColumn`.
- Carried-forward blocker (unchanged by this plan): CONV-08 (multi-chunk `Table<->pandas` diagnostics-honesty gap, DIAG-01/DIAG-02) remains deferred to a later Phase 2 plan per Phase 1's recorded override.

---
*Phase: 02-full-dtype-structural-coverage*
*Completed: 2026-07-16*
