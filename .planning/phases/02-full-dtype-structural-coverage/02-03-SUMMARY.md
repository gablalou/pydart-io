---
phase: 02-full-dtype-structural-coverage
plan: 03
subsystem: conversion
tags: [rust, pyo3, pandas, arrow, pyarrow, categorical, dictionary, classify_dtype, field-reconstruction, types-mapper]

# Dependency graph
requires:
  - phase: 02-full-dtype-structural-coverage
    plan: 01
    provides: "classify_dtype isinstance-first dispatch skeleton with explicit extension-point comments; FlintError::UnsupportedColumn mapped to flint.FlintError"
  - phase: 02-full-dtype-structural-coverage
    plan: 02
    provides: "ArrowKind::String + plan_column arms; classify_dtype/validate_object_column_contents pattern this plan extends with a distinct Categorical backend/kind pairing"
provides:
  - "DtypeBackend::Categorical + ArrowKind::Categorical + plan_column arm (RequiresCopy, categorical-specific reason) -- OQ2 decision recorded in code"
  - "classify_dtype CategoricalDtype isinstance branch, inserted before the generic ExtensionDtype reject (a CategoricalDtype IS an ExtensionDtype)"
  - "from_pandas build_field helper: Field::new_dictionary + with_dict_is_ordered for DataType::Dictionary columns, sourcing is_ordered from the pandas source dtype's .ordered attribute -- fixes Pitfall 3 (ordered flag previously dropped at the Field level)"
  - "to_pandas per-column-type-aware types_mapper (PyCFunction::new_closure): returns None for pyarrow dictionary types (falls through to pyarrow's real-Categorical reconstruction), pandas.ArrowDtype(t) otherwise -- fixes Pitfall 4 (blanket ArrowDtype mapper previously reconstructed a dictionary column as ArrowDtype, not real Categorical)"
  - "CONV-05 proven end-to-end: ordered flag, category order, and exact code width (int8/int16) survive the round trip; OQ1 recorded (strict stays a no-op for the categorical reconstruction copy)"
affects: [02-04, 02-05]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Field-level metadata (dict_is_ordered) that DataType alone cannot carry is sourced from the pandas source dtype and propagated explicitly via Field::new_dictionary + with_dict_is_ordered, rather than the generic Field::new(.., array.data_type().clone(), ..) every other column uses -- a per-DataType-variant build_field helper, not a blanket rebuild"
    - "PyCFunction::new_closure closures that must satisfy Fn(..) + Send + 'static capture NOTHING from the enclosing scope: the GIL token is obtained inside the body via args.py(), and any Python objects needed (pyarrow.types, pandas.ArrowDtype) are (re-)imported inside the body on each call, never captured via `move`"

key-files:
  created:
    - tests/python/test_categorical.py
  modified:
    - crates/flint-core/src/pandas_plan.rs
    - crates/flint-python/src/pandas.rs
    - crates/flint-python/src/table.rs

key-decisions:
  - "OQ1 (RESEARCH.md Open Question 1): to_pandas's strict parameter stays the existing documented no-op; the categorical-reconstruction copy (pyarrow's own default dictionary reconstruction is not zero-copy for the codes buffer) is an intentional, documented copy, NOT surfaced in copy_report() -- recorded in table.rs doc comments and asserted directly in test_categorical_reconstruction_copy_is_documented, so it is not rediscovered as a surprise gap the way DIAG-01/02 was in Phase 1"
  - "OQ2 (RESEARCH.md Open Question 2): Categorical modeled as a distinct DtypeBackend::Categorical + ArrowKind::Categorical pairing, not folded into the generic RequiresCopy fallback -- so plan_column's pure-Rust unit tests exercise the categorical copy decision directly and ColumnConversionRecord's reason string is categorical-specific rather than a generic 'unsupported' message"
  - "plan_column's match was extended with an explicit catch-all arm covering the 5 structurally-unreachable (backend, kind) pairings introduced by adding a third DtypeBackend/ArrowKind variant (e.g. (Arrow, Categorical), (Categorical, Numeric)) -- kept the match exhaustive and explicit (consistent with this project's established convention of exhaustive matches, not wildcards) rather than adding a bare `_` arm"

requirements-completed: [CONV-05]

coverage:
  - id: D1
    description: "An ordered pandas Categorical round-trips from_pandas -> to_pandas as a real pd.Categorical (dtype == 'category', .cat.ordered == True) with exact category order and values preserved, not an ArrowDtype dictionary column (D-17)"
    requirement: "CONV-05"
    verification:
      - kind: unit
        ref: "tests/python/test_categorical.py#test_ordered_categorical_round_trips_as_real_categorical"
        status: pass
    human_judgment: false
  - id: D2
    description: "An unordered Categorical with a deliberately non-alphabetical category definition order preserves that exact order (and ordered == False) through the round trip (D-17)"
    requirement: "CONV-05"
    verification:
      - kind: unit
        ref: "tests/python/test_categorical.py#test_unordered_categorical_preserves_category_definition_order"
        status: pass
    human_judgment: false
  - id: D3
    description: "Exact integer code width (int8 for <=127 categories, int16 for >255 categories) survives the round trip unchanged, not normalized to a single fixed width (D-18)"
    requirement: "CONV-05"
    verification:
      - kind: unit
        ref: "tests/python/test_categorical.py#test_categorical_code_width_int8_preserved"
        status: pass
      - kind: unit
        ref: "tests/python/test_categorical.py#test_categorical_code_width_int16_preserved"
        status: pass
    human_judgment: false
  - id: D4
    description: "from_pandas preserves the ordered flag at the Field level independent of to_pandas -- a direct PyCapsule export (pa.table(flint_table), no to_pandas call) reports ordered=True on the dictionary field's type (Pitfall 3 root-cause pin, D-17)"
    requirement: "CONV-05"
    verification:
      - kind: unit
        ref: "tests/python/test_categorical.py#test_from_pandas_preserves_ordered_flag_before_to_pandas"
        status: pass
    human_judgment: false
  - id: D5
    description: "OQ1 recorded decision: to_pandas(strict=True) does not raise for a categorical column -- the categorical reconstruction copy is an intentional, documented no-op for strict mode"
    requirement: "CONV-05"
    verification:
      - kind: unit
        ref: "tests/python/test_categorical.py#test_categorical_reconstruction_copy_is_documented"
        status: pass
    human_judgment: false
  - id: D6
    description: "DtypeBackend::Categorical + ArrowKind::Categorical + plan_column arm (RequiresCopy, categorical-specific reason) added with a Rust unit test, no regression to cargo test -p flint-core (OQ2 decision)"
    verification:
      - kind: unit
        ref: "crates/flint-core/src/pandas_plan.rs#plan_column_categorical_requires_copy"
        status: pass
      - kind: unit
        ref: "cargo test -p flint-core (13/13 total, no regression)"
        status: pass
    human_judgment: false
  - id: D7
    description: "Full existing pytest suite stays green after the global to_pandas types_mapper change -- proves the per-column-type-aware closure did not regress numeric/bool/string output for non-dictionary columns"
    verification:
      - kind: unit
        ref: "uv run pytest tests/python -q (48/48 total, no regression)"
        status: pass
    human_judgment: false

duration: 41min
completed: 2026-07-17
status: complete
---

# Phase 2 Plan 03: Categorical Round-Trip Fidelity (CONV-05) Summary

**Fixed two metadata-loss bugs (from_pandas's Field construction silently dropping the dictionary `ordered` flag, and to_pandas's blanket `ArrowDtype` mapper reconstructing dictionaries instead of real `Categorical`) so pandas `Categorical` columns round-trip with exact ordered flag, category order, and integer code width fidelity.**

## Performance

- **Duration:** ~41 min
- **Started:** 2026-07-17T13:27:02+08:00 (Task 1 commit)
- **Completed:** 2026-07-17T14:08:49+08:00 (Task 3 commit)
- **Tasks:** 3
- **Files modified:** 4 (1 created, 3 modified)

## Accomplishments

- `DtypeBackend::Categorical` and `ArrowKind::Categorical` added to `crates/flint-core/src/pandas_plan.rs`'s matrix, with a `plan_column` arm `(Categorical, Categorical) => RequiresCopy` carrying a categorical-specific reason (a Categorical's split codes+categories representation has no single flat Arrow-compatible buffer). Module doc matrix table updated; a new `plan_column_categorical_requires_copy` unit test added. The match was extended with an explicit catch-all arm for the 5 structurally-unreachable `(backend, kind)` pairings a third enum variant on each side introduces, keeping the match exhaustive without a bare `_` wildcard (matching this project's established convention).
- `classify_dtype` (`crates/flint-python/src/pandas.rs`) now intercepts `isinstance(dtype, pandas.CategoricalDtype)` BEFORE the generic `ExtensionDtype` reject branch -- a `CategoricalDtype` IS an `ExtensionDtype`, so placing the check after the catch-all would have made it unreachable and every `Categorical` column would have been incorrectly rejected as an unsupported masked extension dtype. `from_pandas`'s `is_contiguous` match gained a `DtypeBackend::Categorical => true` arm (never touches `.values.flags`; categoricals always route through the stream fallback).
- **Pitfall 3 fixed:** `from_pandas`'s Field-construction loop no longer unconditionally calls `Field::new(&name, array.data_type().clone(), ..)` for every column. A new `build_field` helper special-cases `DataType::Dictionary` columns via `Field::new_dictionary(..).with_dict_is_ordered(is_ordered)`, sourcing `is_ordered` from the pandas source dtype's own `.ordered` attribute for `DtypeBackend::Categorical` columns. Non-dictionary columns are unaffected (same `Field::new` path as before). Verified empirically before and after: a direct `pa.table(flint_table)` export (no `to_pandas` call) now reports `ordered=True` for an `ordered=True` source Categorical, where it previously reported `ordered=0` regardless of source.
- **Pitfall 4 fixed:** `to_pandas`'s blanket `kwargs.set_item("types_mapper", arrow_dtype)` (a static class reference) was replaced with a `PyCFunction::new_closure` that returns `None` for `pyarrow.types.is_dictionary` columns (falling through to pyarrow's own default, non-ArrowDtype reconstruction -- which produces a real `pd.Categorical`) and `pandas.ArrowDtype(t)` for every other column (unchanged Phase 1/Plan 01-02 behavior). The closure captures nothing from the enclosing scope (obtains the GIL token from its own `args`, re-imports `pyarrow.types`/`pandas.ArrowDtype` inside the body on each call) to satisfy `PyCFunction::new_closure`'s `F: Fn(..) -> R + Send + 'static` bound -- the RESEARCH.md sketch's `move`-capturing shape does not compile for this reason.
- `tests/python/test_categorical.py` created with 6 tests: ordered round-trip (D-17), unordered category-order preservation (D-17), int8 code width (D-18), int16 code width for >255 categories (D-18), direct-PyCapsule-export ordered-flag pin (Pitfall 3 root cause, independent of `to_pandas`), and OQ1's `strict=True` no-op assertion.
- Doc comments in `table.rs` (module-level and `to_pandas` method) updated to record OQ1: `strict` stays a documented no-op; the categorical codes-buffer reconstruction copy is intentional and not surfaced in `copy_report()`.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add DtypeBackend::Categorical + ArrowKind::Categorical (matrix + unit tests) and classify_dtype categorical branch** - `134e9a5` (feat)
2. **Task 2: Fix from_pandas Field dict_is_ordered propagation (Pitfall 3) AND to_pandas per-column types_mapper (Pitfall 4)** - `23f607e` (fix)
3. **Task 3: Python fidelity tests for CONV-05** - `b0637e3` (test)

_Note: this SUMMARY's own metadata commit is created separately per worktree execution rules (STATE.md/ROADMAP.md are NOT updated here -- the orchestrator owns those after all wave agents complete)._

## Files Created/Modified

- `crates/flint-core/src/pandas_plan.rs` - `DtypeBackend::Categorical` + `ArrowKind::Categorical` variants, `plan_column` arm + exhaustive catch-all for unreachable pairings, module doc matrix table update, `plan_column_categorical_requires_copy` unit test
- `crates/flint-python/src/pandas.rs` - `classify_dtype` gains a `CategoricalDtype` isinstance branch (before the generic `ExtensionDtype` reject); `from_pandas`'s `is_contiguous` match gains a `Categorical => true` arm; new `build_field` helper replacing the unconditional `Field::new(.., array.data_type().clone(), ..)` call, propagating `dict_is_ordered` for `DataType::Dictionary` columns
- `crates/flint-python/src/table.rs` - `to_pandas`'s blanket `types_mapper=pandas.ArrowDtype` replaced with a per-column-type-aware `PyCFunction::new_closure`; doc comments updated to record OQ1
- `tests/python/test_categorical.py` - new: CONV-05 tests (6 tests: ordered/unordered round-trip, int8/int16 code width, direct-PyCapsule-export ordered-flag pin, OQ1 strict no-op)

## Decisions Made

- **OQ1 (recorded):** `to_pandas`'s `strict` parameter stays the existing documented no-op; the categorical reconstruction copy is intentional and documented, not surfaced in `copy_report()`. Asserted directly in `test_categorical_reconstruction_copy_is_documented`.
- **OQ2 (recorded):** `Categorical` modeled as a distinct `DtypeBackend`/`ArrowKind` pairing (not folded into the generic `RequiresCopy` fallback), so `plan_column`'s pure-Rust unit tests exercise the categorical decision directly with a categorical-specific reason string.
- The `plan_column` match was kept fully exhaustive (an explicit catch-all arm listing all 5 unreachable pairings, not a bare `_`), consistent with this repo's established convention (per `git log`, a prior commit explicitly hardened this file's matches to be exhaustive rather than relying on wildcards).

## Deviations from Plan

None - plan executed exactly as written. The advisor-flagged risk (the RESEARCH.md `types_mapper` sketch's `move`-capturing closure shape does not compile under `PyCFunction::new_closure`'s `Send + 'static` bound) was anticipated by the plan itself (its Task 2 action explicitly calls this out as a "Python-verified call-site SKETCH, not compilable Rust" and specifies the capture-nothing shape), so implementing it correctly on the first attempt is plan-conformant, not a deviation.

## Issues Encountered

None beyond the compile-shape consideration documented above (anticipated by the plan, not a surprise).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `classify_dtype`'s isinstance-first dispatch now correctly distinguishes `CategoricalDtype` from the generic `ExtensionDtype` reject; the extension-point comment for Plan 04 (timestamp/duration, DatetimeTZDtype-if-applicable) is updated and still points to the correct insertion location.
- `build_field`'s per-`DataType`-variant construction pattern (special-casing `DataType::Dictionary` while leaving every other variant on the generic `Field::new` path) establishes the pattern for any future column type whose Field-level metadata `DataType` alone cannot carry.
- The `PyCFunction::new_closure` capture-nothing pattern (GIL token from `args.py()`, re-import inside the body) is now demonstrated working end-to-end in this codebase -- reusable if a later plan needs another per-column-type-aware Python callable at the PyO3 boundary.
- Carried-forward blocker (unchanged by this plan): CONV-08 (multi-chunk `Table<->pandas` diagnostics-honesty gap, DIAG-01/DIAG-02) remains deferred per Phase 1's recorded override, tracked for Plan 05.

---
*Phase: 02-full-dtype-structural-coverage*
*Completed: 2026-07-17*
