---
phase: 03-parquet-io
plan: 03
subsystem: io
tags: [rust, pyo3, arrow-rs, parquet, predicate-pushdown, row-group-pruning, projection]

# Dependency graph
requires:
  - phase: 03-parquet-io Plan 01
    provides: "flint_core::parquet_io::{write_parquet, read_parquet} (single-batch signature), Table.from_parquet/to_parquet #[pymethods], pyo3-free core/PyO3-boundary split"
  - phase: 03-parquet-io Plan 02
    provides: "build_writer_properties (four-codec map + row-count row-group size), to_parquet's compression/row_group_size parameters -- row-group statistics this plan's pruning reads are written by this plan's own writer path"
provides:
  - "flint_core::parquet_filter::{Op, ScalarValue, FilterExpr, could_match_range}: pyo3-free, exhaustively unit-tested six-operator range-comparison logic (the != asymmetric single-valued-group rule as its own explicit match arm)"
  - "flint_core::parquet_io::surviving_row_groups(metadata, arrow_schema, parquet_schema, filters) -> Vec<usize>: row-group-level skip decision via StatisticsConverter + could_match_range, AND-combined across multiple filters"
  - "flint_core::parquet_io::read_parquet extended to accept projection (Option<&[String]>) + &[FilterExpr]: builds RowFilter/ArrowPredicateFn from the SAME parsed filter list surviving_row_groups consumes, builds an independent output ProjectionMask, reorders the decoded batch to the caller's exact requested column order"
  - "Table.from_parquet(path, columns=None, filters=None): parses filters=[(column, operator, value), ...] 3-tuples ONCE at the PyO3 boundary (operator string -> Op via exhaustive match; value -> ScalarValue via bool-before-int-before-float-before-str extraction) and delegates to the extended read_parquet"
  - "FlintError::UnsupportedFilterOperator{column, operator} routed through PyFlintError::new_err -- an operator outside the six D-25 strings raises a named, catchable flint.FlintError, never a silently skipped tuple"
  - "tests/rust/parquet_row_group_pruning.rs: PARQ-04 skip-engagement integration probe calling the REAL surviving_row_groups against a real three-row-group Parquet file"
  - "tests/python/test_parquet_pushdown.py: 43 tests proving exact-rows-only filtering, six-operator boundary correctness (including the != over-pruning discriminator), projection ordering, projection+filter independence, unknown-operator rejection, empty/no-match edges, and read idempotency"
affects: ["03-parquet-io Plan 04 (multi-file/dtype fidelity work reads through this same read_parquet signature)"]

tech-stack:
  added: []
  patterns:
    - "Single-source-of-truth filter list: table.rs parses filters=[(column, operator, value), ...] into Vec<FilterExpr> exactly once per from_parquet call; surviving_row_groups (row-group skip) and build_row_filter (row-level RowFilter) both consume that SAME slice -- never re-parsed or re-derived between the two consumers (mirrors pandas_plan.rs's existing single-decision-point discipline)."
    - "Row-group pruning is strictly an optimization layered UNDER exact row-level filtering: could_match_range only returns false (skip) when a range provably cannot satisfy the predicate; every other case (including missing/null stats and != on a non-single-valued range) conservatively keeps the group, so RowFilter's exact per-row evaluation is what actually guarantees zero false positives regardless of pruning correctness."
    - "Output ProjectionMask (what columns are physically decoded and returned) is built entirely independently of each filter predicate's own single-column ProjectionMask (what a RowFilter closure evaluates) -- a filter column need not appear in the output, and ProjectionMask::columns' schema-order output is explicitly reordered via RecordBatch::project to match the caller's requested column order."

key-files:
  created:
    - crates/flint-core/src/parquet_filter.rs
    - tests/rust/parquet_row_group_pruning.rs
    - tests/python/test_parquet_pushdown.py
  modified:
    - crates/flint-core/src/parquet_io.rs
    - crates/flint-core/src/lib.rs
    - crates/flint-core/Cargo.toml
    - crates/flint-python/src/table.rs
    - crates/flint-python/src/error.rs

key-decisions:
  - "ScalarValue is a plain, arrow-crate-free enum (Int64/Float64/Bool/Utf8) so could_match_range is unit-testable with zero Arrow array construction; extracting a ScalarValue from a real Arrow statistics array (scalar_from_array) and building literal arrays for RowFilter comparison (scalar_value_to_array + cast) are flint-core's job, kept entirely out of parquet_filter.rs's public surface."
  - "Int64/Float64 cross-type comparisons widen to f64 in parquet_filter::compare, so a Python int filter literal correctly compares against a float64 column's statistics (and vice versa) rather than being conservatively treated as incomparable."
  - "Utf8/LargeUtf8 column statistics are never trusted for row-group pruning (scalar_from_array returns None for them) because Parquet writers may truncate string min/max statistics, which could cause could_match_range to over-prune -- string filters still get exact row-level correctness via RowFilter, they simply never benefit from the row-group-skip IO optimization."
  - "Filter value extraction at the PyO3 boundary checks bool BEFORE int BEFORE float BEFORE str, because Python's bool is a subclass of int -- checking int first would silently coerce True/False filter values into Int64(1)/Int64(0) instead of the intended Bool variant."

patterns-established:
  - "A pyo3-free comparison-logic module (parquet_filter.rs) consumed by exactly one core-crate read-path function, with the PyO3 boundary responsible for all Python-value parsing/extraction and error mapping -- extends pandas_plan.rs's existing pyo3-free-core-logic convention to the read/filter side of the crate."

requirements-completed: [PARQ-04, PARQ-05]

coverage:
  - id: D1
    description: "could_match_range implements all six D-25 operators as an exhaustive match with no wildcard arm; the != operator only proves 'no match possible' when a row group is single-valued and equal to the excluded value, conservatively keeping every other range including min<value<max"
    requirement: PARQ-04
    verification:
      - kind: unit
        ref: "crates/flint-core/src/parquet_filter.rs#tests (14 table-driven cases)"
        status: pass
    human_judgment: false
  - id: D2
    description: "surviving_row_groups genuinely engages row-group-level skipping driven by the file's own written statistics, verified in isolation against a real three-row-group Parquet file (not merely that final rows are correct)"
    requirement: PARQ-04
    verification:
      - kind: integration
        ref: "tests/rust/parquet_row_group_pruning.rs#col_gt_250_keeps_only_last_row_group, #col_lt_50_keeps_only_first_row_group, #col_ge_100_and_lt_200_keeps_only_middle_row_group, #col_eq_1000_keeps_no_row_group, #filter_on_stats_less_column_keeps_all_row_groups"
        status: pass
    human_judgment: false
  - id: D3
    description: "from_parquet(filters=[...]) returns ONLY matching rows across all six operators and boundary value positions (below min, at min, mid-range, at max, above max, and value==single-valued-group), including the != over-pruning discriminator (min<value<max and min==max==value)"
    requirement: PARQ-04
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_pushdown.py#test_operator_coverage_property (36 parametrized cases), #test_single_filter_returns_only_matching_rows, #test_and_combination"
        status: pass
    human_judgment: false
  - id: D4
    description: "columns=[...] projection restricts the returned Table to named columns in the requested order; columns and filters are independently combinable, including a filter column not present in the output projection"
    requirement: PARQ-05
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_pushdown.py#test_projection_returns_subset_in_order, #test_projection_and_filter_combinable_filter_col_not_projected"
        status: pass
    human_judgment: false
  - id: D5
    description: "An operator string outside the six D-25 operators raises flint.FlintError (FlintError::UnsupportedFilterOperator) naming the column and operator, not a silently skipped tuple"
    requirement: PARQ-04
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_pushdown.py#test_unknown_operator_raises"
        status: pass
    human_judgment: false
  - id: D6
    description: "Empty (0-row) files and no-match filters return a 0-row Table rather than an error; repeated identical from_parquet(filters=..., columns=...) reads on the same file are idempotent"
    requirement: PARQ-04
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_pushdown.py#test_filter_on_stats_less_or_empty, #test_idempotent_double_read"
        status: pass
    human_judgment: false

# Metrics
duration: resumed session (mid-task interruption, see Deviations)
completed: 2026-07-24
status: complete
---

# Phase 3 Plan 3: Read-Side Predicate Pushdown + Column Projection Summary

**`from_parquet(columns=[...], filters=[(column, op, value), ...])` with row-group-statistics-driven pruning AND exact row-level filtering, both built from one parsed FilterExpr list, verified across all six operators and boundary/edge cases including the `!=` over-pruning discriminator.**

## Performance

- **Duration:** Resumed session — a prior executor agent was terminated mid-task (Task 2, uncommitted) by a provider session/usage-limit error, not a code failure. This session reviewed the prior agent's uncommitted work, found `parquet_io.rs`/`error.rs` already correct and complete, finished the actual gap (`table.rs` wiring + a required test upgrade), then executed Task 3 to completion.
- **Tasks:** 3/3 completed (Task 1 was already committed before this session began)
- **Files modified:** 5 modified (`parquet_io.rs`, `error.rs`, `table.rs`, `parquet_row_group_pruning.rs`, `lib.rs`/`Cargo.toml` via Task 1), 2 created this session (`parquet_filter.rs` was Task 1; `test_parquet_pushdown.py` new this session)

## Accomplishments
- Row-group-level predicate pushdown genuinely engages: `surviving_row_groups` uses `StatisticsConverter` + `could_match_range` to skip row groups the file's own written min/max statistics prove cannot match, verified in isolation against a real three-row-group Parquet file (not just via correct final row output).
- Exact row-level filtering via `RowFilter`/`ArrowPredicateFn` guarantees zero false positives regardless of pruning correctness — pruning is strictly a layered-under optimization, never a substitute for row-level correctness.
- Column projection (`columns=[...]`) and filtering (`filters=[...]`) are independent and combinable: a filter column need not appear in the output projection, and the output preserves the caller's exact requested column order (not schema order).
- All six D-25 operators (`==`, `!=`, `<`, `<=`, `>`, `>=`) proven correct at boundaries via a 36-case property test comparing Flint's pushdown read against an unfiltered-read-then-pandas-filter baseline, specifically including the `!=` operator's asymmetric single-valued-group skip rule.
- Unknown filter operators raise a named `flint.FlintError` at the PyO3 boundary before any Rust-side filter logic runs — never silently dropped.

## Task Commits

Each task was committed atomically:

1. **Task 1: parquet_filter.rs — typed FilterExpr/Op + could_match_range** - `9f26fe6` (feat) — completed and committed by the prior executor agent before the session interruption; reviewed and left unchanged.
2. **Task 2: Read-path wiring — surviving_row_groups + RowFilter + ProjectionMask; filter-tuple/columns parsing; UnsupportedFilterOperator error** - `46d7dee` (feat) — this session.
3. **Task 3: Python pushdown/projection tests** - `7bbad9d` (test) — this session.

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/flint-core/src/parquet_filter.rs` - `Op`, `ScalarValue`, `FilterExpr`, `could_match_range` (Task 1, unchanged this session)
- `crates/flint-core/src/parquet_io.rs` - `read_parquet` extended with projection + `&[FilterExpr]`; `surviving_row_groups`; `build_row_filter`; `evaluate_predicate`; `scalar_value_to_array`; `scalar_from_array`
- `crates/flint-python/src/error.rs` - `FlintError::UnsupportedFilterOperator{column, operator}` + `PyErr` mapping
- `crates/flint-python/src/table.rs` - `from_parquet(path, columns=None, filters=None)`; `parse_filter_operator`; `parse_filter_value`
- `tests/rust/parquet_row_group_pruning.rs` - upgraded this session to call the real `surviving_row_groups` (was a Task-1 stub calling `could_match_range` directly, per the plan's own explicit instruction that the committed end state must call the real function)
- `tests/python/test_parquet_pushdown.py` - 43 new tests (Task 3)

## Decisions Made
See `key-decisions` in frontmatter: ScalarValue is arrow-crate-free; Int64/Float64 comparisons widen to f64; Utf8 statistics are never trusted for pruning (truncation risk) though still filtered exactly via RowFilter; filter-value extraction checks bool before int before float before str.

## Deviations from Plan

### Auto-fixed Issues

**1. [Resume continuity — not a Rule 1-4 deviation] Completed a mid-task interruption, not a redo**
- **Found during:** Session start (before Task 2)
- **Issue:** A prior executor agent was terminated mid-Task-2 by a provider session/usage-limit error while doing a minor cleanup edit. `parquet_io.rs` and `error.rs` had substantial, high-quality uncommitted work on disk; `table.rs`'s `from_parquet` had not yet been updated, so `cargo build --workspace` failed with a 3-arg-vs-1-arg `E0061`.
- **Fix:** Reviewed the entire uncommitted diff line-by-line against Task 2's `<action>`/`<acceptance_criteria>`. Found `parquet_io.rs` (`surviving_row_groups`, `build_row_filter`, `evaluate_predicate`, projection reordering, doc comments) and `error.rs` (`UnsupportedFilterOperator`) already fully correct and complete — no changes needed. Implemented only the missing piece: `table.rs`'s `from_parquet` signature extension, filter-tuple/operator/value parsing, and delegation to the extended `read_parquet`.
- **Files modified:** `crates/flint-python/src/table.rs`
- **Verification:** `cargo build --workspace` green; `cargo test --workspace` green; `uv run maturin develop && uv run pytest tests/python/test_parquet_roundtrip.py tests/python/test_parquet_compression.py -q` (Task 2's own verify command) green; manual smoke test of `columns=`/`filters=`/AND-combination/unknown-operator behavior confirmed correct end-to-end before committing.
- **Committed in:** `46d7dee`

**2. [Rule 3 - Blocking, explicitly directed by the plan] Upgraded `parquet_row_group_pruning.rs` from a Task-1 stub to call the real `surviving_row_groups`**
- **Found during:** Task 2
- **Issue:** Task 1's committed version of `tests/rust/parquet_row_group_pruning.rs` duplicated the row-group-skip decision loop by calling `could_match_range` directly against `StatisticsConverter` output, because `surviving_row_groups` did not exist until Task 2. The file's own doc comment documented this as an intentional interim state, explicitly directed by 03-03-PLAN.md's Task 1 `<action>`: "if Task 2 is not yet present, stub the test against `could_match_range`'s decision per group and upgrade it in Task 2 — but the committed end state must call the real `surviving_row_groups`."
- **Fix:** Replaced the duplicated per-row-group loop with direct calls to `flint_core::parquet_io::surviving_row_groups`, including changing the AND-combination test (`col_ge_100_and_lt_200_keeps_only_middle_row_group`) to pass both filters together so `surviving_row_groups`' own intersection logic is exercised, rather than intersecting two separately-computed `Vec<usize>` in the test itself.
- **Files modified:** `tests/rust/parquet_row_group_pruning.rs`
- **Verification:** `cargo test -p flint-core` — all 5 tests in this file pass against the real function.
- **Committed in:** `46d7dee`

---

**Total deviations:** 2 (1 resume-continuity review with no code changes required beyond the documented gap; 1 plan-directed test upgrade explicitly anticipated by Task 1's own action text)
**Impact on plan:** No scope creep — both were required to satisfy Task 2's own acceptance criteria and the plan's explicit Task 1 instruction. No architectural changes, no new dependencies.

## Issues Encountered
None beyond the mid-task interruption described above (a provider-side session/usage-limit error, not a code defect) — the prior agent's in-progress work was legitimate and required no correction.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- PARQ-04 and PARQ-05 are complete and independently verified: row-group pruning genuinely engages (isolated Rust integration test), exact row-level filtering guarantees no false positives (Python property test across all six operators + boundaries), and projection/filter are independently combinable.
- `read_parquet`'s signature (`path, columns: Option<&[String]>, filters: &[FilterExpr]`) is now the stable read-path contract Plan 04 (multi-file, dtype fidelity) builds on without needing further signature changes.
- No blockers identified for Plan 04.

---
*Phase: 03-parquet-io*
*Completed: 2026-07-24*
