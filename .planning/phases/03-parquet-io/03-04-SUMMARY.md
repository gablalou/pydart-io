---
phase: 03-parquet-io
plan: 04
subsystem: io
tags: [rust, pyo3, arrow-rs, parquet, dictionary, categorical, timezone, nullability, multi-file]

# Dependency graph
requires:
  - phase: 03-parquet-io Plan 01
    provides: "flint_core::parquet_io::{write_parquet, read_parquet}; Table.from_parquet/to_parquet #[pymethods]; Wave-0 A6 gate proving arrow-rs's default ARROW:schema mechanism preserves DataType::Dictionary/tz strings at the bare-arrow-rs level"
  - phase: 03-parquet-io Plan 03
    provides: "read_parquet(path, projection, filters) single-file signature that this plan's multi-file read_parquet_multi wraps per-file"
  - phase: 02-full-dtype-structural-coverage
    provides: "02-REVIEW.md WR-01 finding (build_field nullability bug) that this plan fixes; Field::new_dictionary/with_dict_is_ordered categorical fidelity mechanism that this plan proves survives Parquet"
provides:
  - "crates/flint-python/src/pandas.rs: build_field(column_name, array, is_ordered, is_nullable) -- nullability now sourced from the declared source pandas dtype (schema.field(0).is_nullable() threaded from import_column_via_pandas_stream), not array.null_count() > 0 (WR-01/D-31 fixed)"
  - "crates/flint-core/src/parquet_io.rs: read_parquet_multi(paths, ...) -- reads N files, asserts strict Arrow-schema equality across all of them before concatenating, returns FlintError::ParquetSchemaMismatch naming the first mismatched file+column on any divergence (D-21)"
  - "crates/flint-python/src/table.rs: from_parquet accepts str/Path (file or directory) or a list of str/Path; directory reads filter to *.parquet sorted lexicographically; empty directory/list raises FlintError::InvalidParquetPathArgument"
  - "crates/flint-python/src/error.rs: FlintError::ParquetSchemaMismatch, FlintError::ParquetReadError, FlintError::InvalidParquetPathArgument variants"
  - "tests/python/test_wr01_nullability.py: direct WR-01 assertion + the exact 02-REVIEW.md concat_tables reproduction, now passing"
  - "tests/python/test_parquet_multifile.py: multi-file/directory read, strict schema-mismatch, empty/single edges, deterministic ordering"
  - "tests/python/test_parquet_fidelity.py: PARQ-06 categorical/dictionary + tz-aware timestamp Parquet round-trip fidelity, scoped to the confirmed arrow-rs DictEncoder reordering gap (see Known Gap below)"
affects: ["Phase 4 (benchmark/release readiness) -- the documented categorical dictionary-ordering gap should be mentioned in release docs/changelog if categoricals are a headline interop claim"]

tech-stack:
  added: []
  patterns:
    - "Declared-schema nullability threading: wherever a column's Arrow field is built from a pandas source, nullability is read from the SOURCE schema's own declared nullability (schema.field(0).is_nullable()), never re-derived from the observed data (array.null_count() > 0) -- observed-data-derived nullability is a narrowing bug waiting to happen (WR-01) since a nullable column can trivially contain zero nulls in one batch."
    - "Strict cross-file schema equality before any multi-file concatenation: read_parquet_multi never unions/best-effort-merges divergent per-file schemas -- first mismatch aborts with the exact file+column named, matching the project's established no-silent-best-effort error family (UnsupportedCodec, UnsupportedFilterOperator, ParquetSchemaMismatch)."
    - "Fidelity tests assert exactly what the underlying re-encoding mechanism guarantees, not what would be nice: where arrow-rs's own encoder measurably diverges from the Arrow-level round-trip semantics (dictionary key/category reassignment), the test suite pins the actual observed behavior instead of asserting an aspirational invariant that would falsely fail (or worse, get silently loosened to keep the suite green)."

key-files:
  created:
    - tests/python/test_wr01_nullability.py
    - tests/python/test_parquet_multifile.py
    - tests/python/test_parquet_fidelity.py
  modified:
    - crates/flint-python/src/pandas.rs
    - crates/flint-core/src/parquet_io.rs
    - crates/flint-python/src/table.rs
    - crates/flint-python/src/error.rs

key-decisions:
  - "build_field's is_nullable parameter is sourced from import_column_via_pandas_stream's own destructured schema (schema.field(0).is_nullable()), which was already being read out of py_table.into_inner() and simply unused -- no new pandas/pyarrow call needed."
  - "borrow_numpy_numeric_column's zero-copy fast path (which does NOT go through import_column_via_pandas_stream) keeps its hard-coded is_nullable=false unchanged -- that contiguous-numpy dtype family cannot represent nulls, so this is correct, not an oversight."
  - "Multi-file schema-mismatch policy is strict-match-required (RESEARCH Open Question 1 resolution): read_parquet_multi never unions or best-effort-merges; the first schema divergence aborts the whole read with a named FlintError rather than silently producing a partially-merged Table."
  - "CHECKPOINT DECISION (Option A, user-approved 2026-07-24): categorical/dictionary Parquet fidelity tests assert only what is empirically guaranteed after Parquet re-encoding -- DataType::Dictionary preserved, dict_is_ordered preserved, per-row values correct -- and deliberately do NOT assert exact .cat.categories order or unused-category retention. See Known Gap section below for the full mechanism and correctness implications."
  - "test_full_dtype_matrix_parquet_round_trip compares columns by value (.tolist()) rather than a single blanket assert_frame_equal, because to_pandas() always reconstructs every column via pyarrow's ArrowDtype types_mapper (established Phase 1 behavior) regardless of the source column's pandas dtype backend -- a numpy-backed source column (plain datetime64[ns]/timedelta64[ns], used here deliberately to exercise that dtype family per the plan) never has a matching pandas dtype *backend* on the round-tripped result even when every value is correct. This is unrelated to the categorical dictionary-ordering gap; it is pre-existing, established conversion behavior discovered while writing this test (Rule 1 test-correctness fix, not a product bug)."

patterns-established:
  - "Declared-schema (not observed-data) nullability sourcing for any future Arrow field construction from a pandas/pyarrow source."
  - "Fidelity/round-trip tests pin actually-observed re-encoder behavior for any documented, accepted gap, rather than silently weakening or omitting the assertion -- so a future dependency upgrade that changes the behavior is caught by a failing test, not by surprise in production."

requirements-completed: [PARQ-01, PARQ-06]

coverage:
  - id: D1
    description: "WR-01/D-31 fixed: build_field derives Arrow field nullability from the declared source pandas dtype (schema.field(0).is_nullable()), not observed null_count() > 0; the exact 02-REVIEW.md concat_tables ArrowInvalid failure is resolved"
    requirement: PARQ-01
    verification:
      - kind: unit
        ref: "tests/python/test_wr01_nullability.py::test_nullable_arrow_dtype_zero_nulls_round_trips_as_nullable_field"
        status: pass
      - kind: unit
        ref: "tests/python/test_wr01_nullability.py::test_concat_tables_across_zero_null_and_nullable_sibling"
        status: pass
    human_judgment: false
  - id: D2
    description: "Multi-file/directory from_parquet (D-21): accepts str/Path (file or directory) or list of str/Path, concatenates into one Table; directory reads filter to *.parquet sorted lexicographically; strict cross-file schema-match (ParquetSchemaMismatch on divergence, never silent union); empty directory/list raises a named FlintError"
    requirement: PARQ-01
    verification:
      - kind: integration
        ref: "tests/python/test_parquet_multifile.py"
        status: pass
    human_judgment: false
  - id: D3
    description: "PARQ-06 categorical/dictionary Parquet fidelity: DataType::Dictionary and dict_is_ordered survive a full flint.Table round-trip, with correct per-row values -- scoped to the confirmed arrow-rs DictEncoder gap (category order / unused categories not guaranteed, see Known Gap)"
    requirement: PARQ-06
    verification:
      - kind: unit
        ref: "tests/python/test_parquet_fidelity.py::test_ordered_categorical_dictionary_survives_parquet_round_trip"
        status: pass
      - kind: unit
        ref: "tests/python/test_parquet_fidelity.py::test_unordered_categorical_dictionary_survives_parquet_round_trip"
        status: pass
      - kind: unit
        ref: "tests/python/test_parquet_fidelity.py::test_single_category_dictionary_round_trip"
        status: pass
      - kind: unit
        ref: "tests/python/test_parquet_fidelity.py::test_multi_category_dictionary_round_trip"
        status: pass
      - kind: unit
        ref: "tests/python/test_parquet_fidelity.py::test_ordered_categorical_category_order_not_guaranteed_known_gap"
        status: pass
    human_judgment: false
  - id: D4
    description: "PARQ-06 tz-aware timestamp fidelity: exact IANA zone string ('America/New_York') survives a Parquet round-trip byte-identically, with unchanged instants and full nanosecond precision retained (no silent truncation to us/ms) at boundary values (epoch, far-future)"
    requirement: PARQ-06
    verification:
      - kind: unit
        ref: "tests/python/test_parquet_fidelity.py::test_tz_aware_timestamp_exact_zone_survives_parquet_round_trip"
        status: pass
      - kind: unit
        ref: "tests/python/test_parquet_fidelity.py::test_timestamp_boundary_and_ns_precision"
        status: pass
    human_judgment: false
  - id: D5
    description: "Phase-completing full dtype matrix: a single Table combining numeric-with-nulls (CONV-03), string (CONV-04), categorical/ordered (CONV-05), tz-aware timestamp (CONV-06), plain datetime64[ns] (CONV-06), and timedelta64[ns]/Duration (CONV-07) round-trips through Parquet with correct per-row values across every column"
    requirement: PARQ-06
    verification:
      - kind: unit
        ref: "tests/python/test_parquet_fidelity.py::test_full_dtype_matrix_parquet_round_trip"
        status: pass
    human_judgment: false

duration: 39min
completed: 2026-07-24
status: complete
---

# Phase 3 Plan 4: WR-01 nullability fix, multi-file Parquet read, PARQ-06 fidelity Summary

**Fixed the WR-01/D-31 nullability bug, added multi-file/directory Parquet reads with strict schema-match, and proved categorical/dictionary + tz-aware timestamp fidelity through a full Parquet round-trip -- with the categorical dictionary-ordering gap explicitly scoped and documented rather than silently asserted away.**

## Performance

- **Duration:** 39 min (13:08 - 13:47 local; includes a checkpoint pause awaiting user decision on Task 3's categorical fidelity scope)
- **Started:** 2026-07-24T05:08:39Z
- **Completed:** 2026-07-24T05:47:36Z
- **Tasks:** 3
- **Files modified:** 7 (4 modified, 3 created)

## Accomplishments

- WR-01/D-31 fixed: `build_field` now derives Arrow field nullability from the pandas source's declared schema nullability, not observed `null_count()` -- the exact `02-REVIEW.md` `pyarrow.concat_tables` `ArrowInvalid` failure is resolved and reproduction-tested.
- D-21 delivered: `Table.from_parquet` reads a single file, a list of files, or a directory of `.parquet` files into one concatenated `Table`, with strict cross-file schema-match (named `ParquetSchemaMismatch` on divergence, never a silent union) and deterministic lexicographic directory ordering.
- PARQ-06 proven end-to-end: categorical/dictionary encoding (with the `ordered` flag) and exact tz-aware timestamp zone strings survive a full `flint.Table` -> Parquet -> `flint.Table` round-trip, across the complete Phase 1-2 established dtype range (numeric-nulls, string, categorical, tz-timestamp, plain datetime64[ns], timedelta64[ns]/Duration).
- The confirmed arrow-rs `DictEncoder` category-reordering/unused-category-drop gap is scoped precisely in both the test suite and this Summary, per the user's Option A checkpoint decision -- not silently asserted away, not left as an unexplained xfail.

## Task Commits

Each task was committed atomically:

1. **Task 1: WR-01/D-31 fix -- thread declared source nullability through `build_field`** - `2d92b51` (fix)
2. **Task 2: Multi-file/directory Parquet read (D-21) with strict schema-match** - `a154ac7` (feat)
3. **Task 3: PARQ-06 fidelity tests -- categorical/dictionary + tz round-trip through Parquet** - `9591e8b` (test)

**Plan metadata:** (this commit, docs: complete plan)

## Files Created/Modified

- `crates/flint-python/src/pandas.rs` - `build_field` gains an explicit `is_nullable: bool` param (replaces `array.null_count() > 0`); `import_column_via_pandas_stream` threads `schema.field(0).is_nullable()` through to the call site; `borrow_numpy_numeric_column` fast path unchanged (`is_nullable=false`)
- `crates/flint-core/src/parquet_io.rs` - `read_parquet_multi` (or equivalent multi-path extension): reads N files, strict schema-equality check before concat, in file-list order
- `crates/flint-python/src/table.rs` - `from_parquet`'s path argument resolves str/Path/list/directory to a `Vec<PathBuf>` at the PyO3 boundary; directory reads filtered+sorted; empty input rejected
- `crates/flint-python/src/error.rs` - `FlintError::ParquetSchemaMismatch { first_file, other_file, column }`, `FlintError::ParquetReadError { path, reason }`, `FlintError::InvalidParquetPathArgument` variants added
- `tests/python/test_wr01_nullability.py` - direct WR-01 nullability assertion + the exact `02-REVIEW.md` `concat_tables` reproduction (now passing)
- `tests/python/test_parquet_multifile.py` - multi-file/directory read, schema-mismatch, empty/single, ordering tests
- `tests/python/test_parquet_fidelity.py` - PARQ-06 categorical/dictionary + tz-aware timestamp fidelity tests, scoped to the confirmed dictionary-reordering gap (8 tests, all passing)

## Decisions Made

See `key-decisions` in frontmatter. The load-bearing one for this resumed session: **the categorical/dictionary fidelity tests assert only what Parquet re-encoding actually guarantees** (dictionary-ness, `ordered` flag, per-row values) and deliberately do not assert `.cat.categories` order or unused-category retention, per the user's Option A checkpoint decision.

## Known Gap: arrow-rs `DictEncoder` reorders/prunes dictionary categories on Parquet write

**Mechanism:** arrow-rs's `ArrowWriter`/`DictEncoder` (parquet 59.1.0) reassigns a dictionary column's internal dictionary keys in **first-occurrence-during-encoding row order**, and **drops any category that appears in zero rows** of the batch being written. This happens entirely inside the Parquet dictionary-page encoding path, independent of Flint's own conversion code.

**What IS preserved** (verified by this plan's tests):
- `DataType::Dictionary` -- the column round-trips as a real dictionary-encoded column, never silently decoded to plain `Utf8`/`Int32` values (RESEARCH Pitfall 1's primary concern).
- `dict_is_ordered` -- the Arrow-level `ordered` flag on the dictionary type survives, and pandas' `.cat.ordered` reconstructs correctly as `True`/`False` to match.
- Per-row values -- every row's actual category label is correct after round-trip, regardless of the internal key/category reassignment.

**What is NOT preserved:**
- `.cat.categories` **order** -- the post-round-trip category list is ordered by first-occurrence-in-rows, not by the original `.cat.categories` sequence. Confirmed empirically: source `categories=["c","b","a"]` with row values `["b","a","c","a"]` round-trips with categories `["b","a","c"]`.
- **Unused categories** -- any category with zero occurrences in the written batch is dropped entirely. Confirmed empirically: a 300-category dictionary with only 3 categories actually used (`"c0"`, `"c299"`, `"c150"`) round-trips with exactly those 3 categories, not the original 300-category superset.

**Verification method:** confirmed with a pure arrow-rs/parquet probe independent of any Flint conversion code (isolating the behavior to the `parquet` crate's `DictEncoder`, not Flint's `from_pandas`/`to_pandas`/`build_field` path). Confirmed no `WriterProperties` knob in parquet 59.1.0 controls or disables this reassignment. Confirmed pyarrow does **not** have this limitation on the equivalent round-trip (categories and their order survive unchanged in pyarrow), so this is a genuine **arrow-rs-vs-pyarrow divergence**, not an inherent Apache Parquet format limitation -- and per this project's CLAUDE.md constraint (arrow-rs-only, no hand-rolled Parquet writer), there is no in-scope fix available in v1.

**Correctness implications:**
- For an **unordered** categorical (`ordered=False`), this is purely **cosmetic**: `.cat.categories` order has no semantic meaning for an unordered categorical, so a reordering (or unused-category drop) does not change any comparison, sort, or equality behavior a user would observe through pandas' own categorical API.
- For an **ordered** categorical (`ordered=True`), this is a **real correctness concern**: pandas' ordered-categorical semantics define `<`/`>`/`sort_values()` behavior by category **position**, not by label. If category order silently changes across a Parquet round-trip, the `<` relationship between two category labels can silently invert or otherwise change post-round-trip, even though every individual row's *label* is still correct. A downstream comparison or sort performed after a `to_parquet`/`from_parquet` round-trip on an ordered categorical could therefore produce a different result than the same comparison/sort performed before the round-trip.
- `tests/python/test_parquet_fidelity.py::test_ordered_categorical_category_order_not_guaranteed_known_gap` pins this exact behavior (`ordered` flag preserved as `True`, but category order provably changed from the source) so a future arrow-rs upgrade that fixes or further changes this behavior is caught by a failing test, not silently masked.

**Disposition:** Accepted, documented risk -- not a defect to fix in this plan or phase. No `WriterProperties` mitigation exists in parquet 59.1.0 given the project's arrow-rs-only constraint. If ordered-categorical round-trip correctness becomes a hard product requirement in a future milestone, the only available fixes would be (a) a Flint-side Arrow-level re-sort/relabel step that restores original category order and re-writes affected values after arrow-rs's write path, or (b) waiting on/contributing an upstream arrow-rs fix -- both out of scope here.

### STRIDE Threat Register Addendum

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-03-09 | Tampering (silent semantic change) | arrow-rs `DictEncoder` category reordering on Parquet write (ordered categoricals) | medium | accept | No `WriterProperties` mitigation exists in parquet 59.1.0 (arrow-rs-only constraint, CLAUDE.md); pyarrow does not share this limitation, so this is a Flint-vs-pyarrow-parity gap to disclose in docs/release notes if categoricals are a headline interop claim (Phase 4). Regression-pinned by `test_ordered_categorical_category_order_not_guaranteed_known_gap`. Cosmetic (no behavior change) for unordered categoricals -- only ordered categoricals carry real `<`/sort-order risk. |

This extends the plan's original STRIDE register (T-03-06 through T-03-SC, T-03-02) with the threat surfaced during Task 3 execution that was not anticipated at plan-authoring time.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 4 -> User-resolved checkpoint] Categorical/dictionary fidelity test scope narrowed to what Parquet re-encoding actually guarantees**
- **Found during:** Task 3 (writing `test_parquet_fidelity.py`'s categorical round-trip tests)
- **Issue:** The plan's original acceptance criteria for Task 3 required asserting exact `.cat.categories` order (and, implicitly, full category-pool retention) survives a Parquet round-trip. Empirically, arrow-rs's `ArrowWriter`/`DictEncoder` reassigns dictionary keys in first-occurrence-during-encoding order and drops unused categories -- this is not something Flint's own conversion code controls, and no `parquet` 59.1.0 `WriterProperties` knob disables it. Asserting the plan's original criteria as written would make every categorical fidelity test fail against correct, unfixable-in-scope behavior.
- **Resolution:** Checkpoint raised (architectural/scope decision, Rule 4). User selected Option A (accept as documented limitation): tests assert `DataType::Dictionary` preserved, `dict_is_ordered` preserved, and per-row values correct; they do NOT assert exact category order or unused-category retention. Added a dedicated test (`test_ordered_categorical_category_order_not_guaranteed_known_gap`) that pins the actual observed reordering behavior as a regression detector, plus the "Known Gap" section above and a new STRIDE register entry (T-03-09).
- **Files modified:** `tests/python/test_parquet_fidelity.py`
- **Verification:** All 8 tests in `test_parquet_fidelity.py` pass; full `cargo test --workspace` (11 tests) and `uv run pytest tests/python` (141 tests) both green, no regressions.
- **Committed in:** `9591e8b` (Task 3 commit)

**2. [Rule 1 - Test correctness bug] `test_full_dtype_matrix_parquet_round_trip` switched from blanket `assert_frame_equal` to per-column value comparison**
- **Found during:** Task 3 (writing the full-dtype-matrix test)
- **Issue:** The plan's original test design used a single `pdt.assert_frame_equal(result_df, df)` across all six columns. This fails independently of the categorical gap above: `to_pandas()` always reconstructs every column via pyarrow's `ArrowDtype` types_mapper (established Phase 1 behavior -- see `test_datetime_timedelta.py`'s existing `.tolist()`-based comparison convention), so the plan's intentionally numpy-backed `plain_dt`/`timedelta` source columns never match the round-tripped result's pandas dtype *backend*, even though every value is correct. `assert_frame_equal`'s default dtype-strictness makes this a false failure unrelated to any real fidelity gap.
- **Fix:** Compare `num`/`str`/`tz_ts`/`plain_dt`/`delta` columns via `.tolist()` value equality (matching the codebase's established convention in `test_datetime_timedelta.py` and other tests using numpy-backed source columns), and compare the `cat` column separately per Deviation #1's scoping. Full assert_frame_equal is retained only where every column is genuinely ArrowDtype-consistent between source and result (`test_parquet_roundtrip.py`'s existing pattern), which does not apply to this mixed-dtype-family test.
- **Files modified:** `tests/python/test_parquet_fidelity.py`
- **Verification:** `test_full_dtype_matrix_parquet_round_trip` passes; confirmed via a standalone probe that `to_pandas()` returns `timestamp[ns][pyarrow]`/`duration[ns][pyarrow]` dtype backends for numpy-sourced `datetime64[ns]`/`timedelta64[ns]` columns, matching established, intentional Phase 1 conversion behavior (not a bug).
- **Committed in:** `9591e8b` (Task 3 commit)

---

**Total deviations:** 2 auto-fixed/checkpoint-resolved (1 user-resolved scope checkpoint via Rule 4, 1 Rule 1 test-correctness fix)
**Impact on plan:** Both were necessary to make Task 3's tests reflect actual, correct system behavior rather than assert against an unfixable-in-scope arrow-rs encoder detail or an already-established (and correct) `to_pandas()` dtype-backend convention. No scope creep into product code -- both changes are confined to test assertions and documentation.

## Issues Encountered

None beyond the two deviations documented above, both resolved during Task 3 with no open questions remaining.

## User Setup Required

None -- no external service configuration required.

## Next Phase Readiness

- Phase 3 (parquet-io) is now feature-complete: PARQ-01 (basic + multi-file read), PARQ-02 through PARQ-05 (write config, filter pushdown, projection -- Plans 01-03), and PARQ-06 (logical-type fidelity) are all delivered and tested.
- `cargo test --workspace` (11 tests) and `uv run pytest tests/python` (141 tests) are fully green with zero regressions.
- The documented categorical dictionary-ordering/unused-category gap (Known Gap section above, STRIDE T-03-09) should be surfaced in Phase 4's release/benchmark documentation if categorical Parquet fidelity is presented as a headline interop claim relative to pyarrow -- this is the one place Flint's Parquet round-trip provably diverges from pyarrow's.
- No blockers for Phase 4 (Benchmark & Release Readiness).

---
*Phase: 03-parquet-io*
*Completed: 2026-07-24*

## Self-Check: PASSED

All 8 files/artifacts referenced in this Summary confirmed present on disk (tests/python/test_wr01_nullability.py, tests/python/test_parquet_multifile.py, tests/python/test_parquet_fidelity.py, crates/flint-python/src/pandas.rs, crates/flint-core/src/parquet_io.rs, crates/flint-python/src/table.rs, crates/flint-python/src/error.rs, this SUMMARY.md). All 3 task commits confirmed present in git log (2d92b51, a154ac7, 9591e8b). `cargo test --workspace` (11 tests) and `uv run pytest tests/python -q` (141 tests) both green at time of writing.
