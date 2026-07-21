---
phase: 02-full-dtype-structural-coverage
plan: 04
subsystem: conversion
tags: [rust, pyo3, pandas, arrow, pyarrow, datetime, timezone, timedelta, classify_dtype, ns-resolution]

# Dependency graph
requires:
  - phase: 02-full-dtype-structural-coverage
    plan: 01
    provides: "classify_dtype isinstance-first dispatch skeleton with explicit extension-point comments for the ArrowDtype sub-kind branch and the pre-generic-reject ExtensionDtype insertion point; FlintError::UnsupportedColumn mapped to flint.FlintError"
  - phase: 02-full-dtype-structural-coverage
    plan: 03
    provides: "build_field/CategoricalDtype-before-generic-reject ordering pattern this plan's DatetimeTZDtype branch reuses"
provides:
  - "ArrowKind::Timestamp { tz: Option<String> } + ArrowKind::Duration variants; ArrowKind loses its Copy derive (keeps Clone); plan_column arms for both DtypeBackend::Arrow (ZeroCopyBorrow) and DtypeBackend::Numpy (RequiresCopy, honest reasons)"
  - "classify_dtype extended with three ns-gated temporal entry points: ArrowDtype timestamp/duration (pa.types.is_timestamp/is_duration), pandas.DatetimeTZDtype (before the generic ExtensionDtype reject, routed to DtypeBackend::Numpy for diagnostics honesty), and plain-numpy 'M'/'m' kinds via np.datetime_data"
  - "non_ns_temporal_rejection_reason: a shared, actionable FlintError::UnsupportedColumn reason string naming the pandas-3.0 default-resolution change and suggesting .astype('datetime64[ns]')"
  - "CONV-06/CONV-07 proven end-to-end: datetime64[ns]/datetime64[ns,tz]/timedelta64[ns] round-trip correctly, tz string round-trips as-is (no UTC normalization), and non-ns resolutions (including the realistic pd.to_datetime()-no-explicit-dtype case) are rejected with the actionable message"
affects: [02-05]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "ArrowKind's Timestamp variant carries an owned String (tz), so the enum's Copy derive was removed project-wide -- call sites that used to rely on implicit copies now explicitly .clone() at the one point ArrowKind is consumed by value (plan_column) and borrow (&arrow_kind) at every other use site, rather than changing plan_column's signature to take a reference"
    - "np.datetime_data(dtype) used for numpy datetime64/timedelta64 resolution introspection, never str(dtype) parsing; pandas.DatetimeTZDtype's own .unit attribute used for the tz-aware ExtensionDtype case -- matches RESEARCH.md's verified datetime_unit helper exactly"
    - "A tz-aware DatetimeTZDtype column is classified as DtypeBackend::Numpy (not Arrow), even though it's a pandas ExtensionDtype -- because it is not actually pyarrow-backed memory, so plan_column's RequiresCopy result stays honest rather than falsely claiming zero-copy"

key-files:
  created:
    - tests/python/test_datetime_timedelta.py
  modified:
    - crates/flint-core/src/pandas_plan.rs
    - crates/flint-python/src/pandas.rs

key-decisions:
  - "DatetimeTZDtype routed to DtypeBackend::Numpy rather than DtypeBackend::Arrow: empirically confirmed DatetimeTZDtype is not pyarrow-backed (isinstance(dtype, pandas.ArrowDtype) is False for it), so classifying it as Arrow would make plan_column falsely report ZeroCopyBorrow for a column that in fact requires a real copy through the stream fallback -- Numpy keeps the diagnostics honest, consistent with the project's copy_report()/strict-mode honesty commitment (D-03/D-04)."
  - "Empirically verified before writing classify_dtype's is_contiguous access path is safe for DatetimeTZDtype: Series.values for a tz-aware column returns a plain numpy ndarray (tz-normalized, not the DatetimeArray extension array pandas.Series.array would return), which DOES expose .flags -- so the existing DtypeBackend::Numpy branch's `.values.flags.c_contiguous` access does not crash the way the masked-extension AttributeError (D-08/Pitfall 1) did. Confirmed via direct python -c reproduction against this repo's pinned pandas 3.0.3 before relying on it."
  - "Shared non_ns_temporal_rejection_reason() helper used by all three ns-gated entry points (ArrowDtype timestamp/duration, DatetimeTZDtype, plain-numpy M/m) so the pandas-3.0 actionable message text is defined once, not duplicated three times."

requirements-completed: [CONV-06, CONV-07]

coverage:
  - id: D1
    description: "A datetime64[ns] column round-trips from_pandas -> to_pandas with correct values (D-15/CONV-06)"
    requirement: "CONV-06"
    verification:
      - kind: unit
        ref: "tests/python/test_datetime_timedelta.py#test_datetime_ns_round_trips"
        status: pass
    human_judgment: false
  - id: D2
    description: "A tz-aware datetime64[ns, tz] column round-trips values AND the exact tz string survives with no UTC normalization (D-16)"
    requirement: "CONV-06"
    verification:
      - kind: unit
        ref: "tests/python/test_datetime_timedelta.py#test_tz_aware_datetime_ns_round_trips_tz_as_is"
        status: pass
    human_judgment: false
  - id: D3
    description: "A timedelta64[ns] column round-trips from_pandas -> to_pandas with correct values (D-15/CONV-07)"
    requirement: "CONV-07"
    verification:
      - kind: unit
        ref: "tests/python/test_datetime_timedelta.py#test_timedelta_ns_round_trips"
        status: pass
    human_judgment: false
  - id: D4
    description: "A non-ns-resolution datetime64[us] column (explicit dtype) is rejected with an actionable flint.FlintError naming the column, resolution, pandas-3.0 explanation, and .astype fix"
    requirement: "CONV-06"
    verification:
      - kind: unit
        ref: "tests/python/test_datetime_timedelta.py#test_non_ns_datetime_us_rejected_with_pandas3_message"
        status: pass
    human_judgment: false
  - id: D5
    description: "The realistic pandas-3.0 failure mode -- pd.to_datetime([...]) with NO explicit dtype (yields us resolution by default) -- is rejected with the same actionable message, per RESEARCH.md Pitfall 5's warning-signs requirement"
    requirement: "CONV-06"
    verification:
      - kind: unit
        ref: "tests/python/test_datetime_timedelta.py#test_pd_to_datetime_default_resolution_rejected"
        status: pass
    human_judgment: false
  - id: D6
    description: "A non-ns-resolution timedelta64[us] column is rejected with the analogous actionable message"
    requirement: "CONV-07"
    verification:
      - kind: unit
        ref: "tests/python/test_datetime_timedelta.py#test_non_ns_timedelta_us_rejected"
        status: pass
    human_judgment: false
  - id: D7
    description: "ArrowKind::Timestamp{tz}/Duration variants added with plan_column arms for both Arrow (ZeroCopyBorrow) and Numpy (RequiresCopy) backends, exhaustive catch-all extended, no regression to cargo test -p flint-core"
    verification:
      - kind: unit
        ref: "crates/flint-core/src/pandas_plan.rs#plan_column_arrow_timestamp_is_zero_copy_borrow, plan_column_arrow_duration_is_zero_copy_borrow, plan_column_numpy_timestamp_requires_copy, plan_column_numpy_duration_requires_copy"
        status: pass
      - kind: unit
        ref: "cargo test -p flint-core (17/17 total: 12 pandas_plan unit tests + 3 concat_generic_arrays + 2 zero_copy_alloc, no regression)"
        status: pass
    human_judgment: false
  - id: D8
    description: "Full existing pytest suite stays green after classify_dtype's temporal extension and the ArrowKind Copy-removal call-site fixes"
    verification:
      - kind: unit
        ref: "uv run pytest tests/python -q (55/55 total: 48 pre-existing + 7 new, no regression)"
        status: pass
    human_judgment: false

duration: ~46min (across Task 1/2/3 commit timestamps)
completed: 2026-07-17
status: complete
---

# Phase 2 Plan 04: Datetime, Timezone & Timedelta Round-Trip Fidelity (CONV-06/CONV-07) Summary

**Extended `classify_dtype` with three ns-gated temporal entry points (ArrowDtype timestamp/duration, `pandas.DatetimeTZDtype`, plain-numpy `datetime64`/`timedelta64`) so `datetime64[ns]`, tz-aware `datetime64[ns, tz]`, and `timedelta64[ns]` columns round-trip correctly while any non-ns resolution -- including the realistic pandas-3.0 `pd.to_datetime()`-default failure mode -- is rejected with an actionable error.**

## Performance

- **Duration:** ~46 min (Task 1 commit 16:43:59+08:00 -> Task 3 commit 21:29:32+08:00; the compile-and-verify work itself was concentrated at the start and end of this window)
- **Started:** 2026-07-17T16:43:59+08:00 (Task 1 commit)
- **Completed:** 2026-07-17T21:29:32+08:00 (Task 3 commit)
- **Tasks:** 3
- **Files modified:** 3 (1 created, 2 modified)

## Accomplishments

- `ArrowKind` (`crates/flint-core/src/pandas_plan.rs`) gains `Timestamp { tz: Option<String> }` and `Duration` variants. Because `Timestamp` carries an owned `String`, `ArrowKind` no longer derives `Copy` (keeps `Clone`, `Debug`, `PartialEq`, `Eq`, `Hash`) -- the one call site that consumed `ArrowKind` by value (`plan_column` in `from_pandas`) now clones it explicitly, and the two remaining use sites (`matches!` and the final conversion-strategy `match`) borrow it (`&arrow_kind`) instead, relying on Rust's match ergonomics rather than changing `plan_column`'s own signature.
- `plan_column`'s matrix gained explicit arms: `(Arrow, Timestamp{..})` and `(Arrow, Duration)` -> `ZeroCopyBorrow` (already-Arrow-memory columns, same reasoning as the existing Numeric/Bool/String Arrow arms); `(Numpy, Timestamp{..})` and `(Numpy, Duration)` -> `RequiresCopy` with an honest, temporal-specific reason (numpy `datetime64`/`timedelta64` storage is never an Arrow-compatible buffer). The exhaustive catch-all arm was extended to cover the 2 new structurally-unreachable `(Categorical, Timestamp/Duration)` pairings, keeping the match explicit rather than adding a bare `_` (matching this repo's established convention, confirmed in Plan 03's SUMMARY). 4 new unit tests cover all 4 new arms; `cargo test -p flint-core` stays green (17/17).
- `classify_dtype` (`crates/flint-python/src/pandas.rs`) gained three ns-gated temporal entry points, in dispatch-order-correct positions:
  1. **ArrowDtype branch:** `pyarrow.types.is_timestamp`/`is_duration` on `dtype.pyarrow_dtype`, each ns-gated via `.unit`; timestamp additionally reads `.tz` (`Option<String>`, extracted directly -- `None` for naive, `Some(tz_str)` for tz-aware).
  2. **New `pandas.DatetimeTZDtype` isinstance branch**, inserted BEFORE the generic `ExtensionDtype` reject (same ordering rule as Plan 03's `CategoricalDtype` -- a `DatetimeTZDtype` IS an `ExtensionDtype`, so placing this check later would make it unreachable). Reads `dtype.unit` directly (pandas' `ExtensionDtype` exposes it) for the ns gate, and `str(dtype.tz)` for the tz string (D-16: confirmed empirically this yields the exact original zone name, e.g. `"America/New_York"`, not a UTC-normalized form). Classified as `DtypeBackend::Numpy` (not `Arrow`) -- a deliberate honesty decision, see Decisions Made below.
  3. **Plain-numpy `'M'`/`'m'` kind branch**, using `np.datetime_data(dtype)` (NOT `str(dtype)` parsing, per RESEARCH.md's verified helper) for the ns gate.

  All three entry points funnel non-ns resolutions through a new shared `non_ns_temporal_rejection_reason()` helper, producing a `FlintError::UnsupportedColumn` whose message names the column, the actual resolution, explicitly states that pandas 3.0 changed `pd.to_datetime()`/`pd.to_timedelta()`'s default parsing resolution from nanoseconds to microseconds, and suggests `.astype('datetime64[ns]')` (RESEARCH.md Pitfall 5).
- `tests/python/test_datetime_timedelta.py` created with 7 tests: ns datetime round-trip, tz-aware ns round-trip with exact-tz-string assertion (via the reconstructed `ArrowDtype`'s `pyarrow_dtype.tz`, since `to_pandas` reconstructs via `types_mapper=ArrowDtype`, not `DatetimeTZDtype`), ns timedelta round-trip, explicit `datetime64[us]` rejection, the realistic no-explicit-dtype `pd.to_datetime()` rejection (Pitfall 5 "warning signs" compliance), `timedelta64[us]` rejection, and a combined datetime+timedelta sanity check. Full pytest suite stays green (55/55: 48 pre-existing + 7 new).

## Task Commits

Each task was committed atomically:

1. **Task 1: Add ArrowKind::Timestamp{tz}/Duration + plan_column arms (matrix + unit tests)** - `9b3eb3f` (feat)
2. **Task 2: classify_dtype temporal entry points with ns-only gating + pandas-3.0 rejection message** - `5e07abb` (feat)
3. **Task 3: Python tests for CONV-06/CONV-07** - `bcd5003` (test)

_Note: this SUMMARY's own metadata commit is created separately per worktree execution rules (STATE.md/ROADMAP.md are NOT updated here -- the orchestrator owns those after all wave agents complete)._

## Files Created/Modified

- `crates/flint-core/src/pandas_plan.rs` - `ArrowKind::Timestamp { tz: Option<String> }` + `ArrowKind::Duration` variants (Copy derive removed, Clone kept); `plan_column` arms for `(Arrow, Timestamp/Duration)` -> `ZeroCopyBorrow` and `(Numpy, Timestamp/Duration)` -> `RequiresCopy`; exhaustive catch-all extended; module doc matrix table + `is_contiguous` doc comment updated; 4 new unit tests
- `crates/flint-python/src/pandas.rs` - `classify_dtype` gains 3 ns-gated temporal entry points (ArrowDtype timestamp/duration, new `DatetimeTZDtype` branch before the generic reject, plain-numpy `'M'`/`'m'` kinds); new `non_ns_temporal_rejection_reason()` helper; `from_pandas` threads a new `datetime_tz_dtype_type` parameter through `classify_dtype` and fixes 3 call sites for `ArrowKind` losing `Copy` (clone into `plan_column`, borrow at the 2 remaining use sites)
- `tests/python/test_datetime_timedelta.py` - new: CONV-06/CONV-07 tests (7 tests: ns datetime/timedelta/tz-aware round-trips, explicit + realistic-default-resolution rejections, combined-columns sanity check)

## Decisions Made

- **`DatetimeTZDtype` classified as `DtypeBackend::Numpy`, not `Arrow`.** Empirically confirmed `isinstance(pd.DatetimeTZDtype(...), pandas.ArrowDtype)` is `False` -- a tz-aware `datetime64[ns, tz]` column is genuinely not pyarrow-backed memory. Classifying it as `Arrow` would make `plan_column` falsely report `ZeroCopyBorrow` for a column that in fact requires a real copy through the `import_column_via_pandas_stream` fallback, violating this project's `copy_report()`/strict-mode honesty commitment (D-03/D-04, the same principle Plan 01 fixed for masked extension dtypes).
- **Verified `is_contiguous`'s existing `.values.flags.c_contiguous` access path is safe for `DatetimeTZDtype` before relying on it (not merely assumed).** `Series.values` for a tz-aware column returns a plain numpy `ndarray` (UTC-converted, tz stripped -- distinct from `Series.array`'s `DatetimeArray` extension array), which DOES expose `.flags`. Confirmed via direct `python -c` reproduction against this repo's pinned pandas 3.0.3 before writing the classification branch, avoiding a repeat of the D-08/Pitfall 1-style `AttributeError` crash pattern for a different extension dtype.
- **Shared `non_ns_temporal_rejection_reason()` helper** used by all three ns-gated entry points so the pandas-3.0 actionable message text (naming the resolution, explaining the pandas-3.0 default-resolution change, suggesting `.astype('datetime64[ns]')`) is defined exactly once, not duplicated three times across the ArrowDtype/DatetimeTZDtype/plain-numpy branches.
- **tz fidelity asserted via the reconstructed column's `ArrowDtype.pyarrow_dtype.tz`, not a `pandas.DatetimeTZDtype`.** `to_pandas` reconstructs every column via `types_mapper=pandas.ArrowDtype` (Phase 1/Plan 01-03's established, unchanged mechanism) -- a tz-aware output column is therefore an `ArrowDtype` wrapping a pyarrow `timestamp[ns, tz=...]` type, not a `pandas.DatetimeTZDtype`. Verified empirically (`type(result['a'].dtype) == pandas.ArrowDtype`) before writing the test assertion, so `test_tz_aware_datetime_ns_round_trips_tz_as_is` checks `str(result["a"].dtype.pyarrow_dtype.tz)`, the field that actually carries the tz string on the real reconstructed dtype.

## Deviations from Plan

None - plan executed exactly as written. The plan's own Task 1 action anticipated and explicitly called out the `ArrowKind` Copy-removal call-site fixes (clone/borrow at `from_pandas`'s match sites) as required work, not a discovered deviation; implementing it as specified is plan-conformant.

## Issues Encountered

None. Two empirical pre-checks were run before writing code (not after encountering a failure): confirming `DatetimeTZDtype.values` exposes `.flags` (avoiding a hypothetical repeat of Plan 01's masked-extension crash pattern), and confirming `to_pandas`'s reconstructed tz-aware column is `ArrowDtype`-typed (not `DatetimeTZDtype`) so the round-trip test asserts the tz string against the correct attribute path. Both were anticipated design questions, not surprises encountered mid-task.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `classify_dtype`'s isinstance-first dispatch is now fully populated for this phase's dtype scope: `ArrowDtype` (numeric/bool/string/timestamp/duration sub-kinds), `CategoricalDtype`, `DatetimeTZDtype`, generic `ExtensionDtype` reject, and plain-numpy `dtype.kind` (bool/numeric/object/datetime64/timedelta64). No further extension points remain flagged in the dispatch-order doc comment for this phase's requirements.
- `ArrowKind`'s Copy-to-Clone-only transition is complete and all call sites compile cleanly; any future variant added to `ArrowKind` should follow the same clone-at-`plan_column`/borrow-elsewhere pattern established here if it also carries owned data.
- Carried-forward blocker (unchanged by this plan): CONV-08 (multi-chunk `Table<->pandas` diagnostics-honesty gap, DIAG-01/DIAG-02) remains deferred to Plan 05 per Phase 1's recorded override.

---
*Phase: 02-full-dtype-structural-coverage*
*Completed: 2026-07-17*

## Self-Check: PASSED

All created/modified files confirmed present on disk (pandas_plan.rs, pandas.rs,
test_datetime_timedelta.py, this SUMMARY.md). All 3 task commits confirmed present in
`git log --oneline --all` (9b3eb3f, 5e07abb, bcd5003).
