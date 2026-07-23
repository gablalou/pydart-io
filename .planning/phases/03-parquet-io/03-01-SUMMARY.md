---
phase: 03-parquet-io
plan: 01
subsystem: io
tags: [rust, pyo3, arrow-rs, parquet, arrowwriter, parquetrecordbatchreaderbuilder]

# Dependency graph
requires:
  - phase: 01-walking-skeleton
    provides: flint.Table pyclass composing pyo3_arrow::PyTable, PyO3/pyo3-arrow/arrow-rs crate architecture, flint-core/flint-python pyo3-free-core split
  - phase: 02-full-dtype-coverage
    provides: Full numeric/bool/string/categorical/timestamp/duration from_pandas/to_pandas conversion matrix (used to build the Tables this plan writes to/reads from Parquet)
provides:
  - "flint-core/src/parquet_io.rs: pyo3-free write_parquet(&RecordBatch, &Path) / read_parquet(&Path) -> RecordBatch, extensible by Plans 02-04"
  - "Table.from_parquet (@classmethod) / table.to_parquet (instance method) end-to-end Parquet round-trip (PARQ-01, basic PARQ-02/D-28)"
  - "Wave-0 A6 fidelity gate: empirical proof that arrow-rs's default embedded ARROW:schema metadata mechanism preserves DataType::Dictionary(dict_is_ordered) and exact non-UTC tz strings through a bare Parquet write-then-read"
affects: [03-parquet-io Plan 02 (compression/row-group config), 03-parquet-io Plan 03 (predicate pushdown/projection), 03-parquet-io Plan 04 (dtype fidelity, multi-file, WR-01)]

tech-stack:
  added: ["parquet 59.1.0 (apache/arrow-rs, lockstep-pinned with the existing arrow 59.1.0)"]
  patterns:
    - "pyo3-free core / PyO3-facing boundary split extended to Parquet IO: flint-core/src/parquet_io.rs takes/returns only arrow RecordBatch + std::path::Path types; all str/Path extraction happens in flint-python/src/table.rs"
    - "Core-crate errors surfaced as the underlying library's own Result type (parquet::errors::ParquetError) rather than a project-specific error enum, because flint-core cannot depend on flint-python's pyo3-coupled FlintError (would be circular) -- the PyO3 boundary maps it to FlintError::Other at the single delegation point"
    - "Wave-0 verification-gate-before-dependent-work pattern (established in Phase 1's D-06b and this phase's own RESEARCH.md): a standalone tests/rust/ probe empirically settles a high-risk assumption (A6) before any dependent task is built on top of it"

key-files:
  created:
    - crates/flint-core/src/parquet_io.rs
    - tests/rust/parquet_dictionary_tz_roundtrip.rs
    - tests/python/test_parquet_roundtrip.py
  modified:
    - crates/flint-core/Cargo.toml
    - crates/flint-core/src/lib.rs
    - crates/flint-python/src/table.rs

key-decisions:
  - "Wave-0 A6 gate PASSED empirically: arrow-rs 59.1.0's default ArrowWriter/ParquetRecordBatchReaderBuilder behavior (no with_skip_arrow_metadata()/with_schema() overrides) preserves DataType::Dictionary(Int8, Utf8) with dict_is_ordered=true and the exact tz string \"America/New_York\" through a bare write-then-read. Plans 02-04 can rely on the embedded ARROW:schema mechanism for PARQ-06 fidelity without an explicit schema-hint workaround."
  - "parquet_io.rs (flint-core, pyo3-free) returns Result<_, parquet::errors::ParquetError> rather than FlintError -- FlintError lives in flint-python and pulls in pyo3 (PyFlintError), and flint-core cannot depend on flint-python (would be circular). The PyO3 boundary in table.rs maps ParquetError to FlintError::Other(format!(...)) at the from_parquet/to_parquet call sites -- no new FlintError variant added in this plan, per the task's own read_first guidance."
  - "Multiple Parquet row-group batches are concatenated via arrow::compute::concat_batches into a single RecordBatch on read (honest concat, mirrors the CR-01 fix's discipline in pandas.rs) rather than returning only the first batch."
  - "A truly zero-RecordBatch Table (batches.is_empty()) is rejected with a named FlintError::Other on to_parquet, distinct from the empty-table (0-row, single-batch) case which writes successfully. This state is unreachable via any Table-construction path in the current codebase (from_pandas/from_parquet/from_pytable/from_arrow all always yield exactly one RecordBatch, possibly 0 rows) but is guarded defensively with a named error rather than an unguarded panic, consistent with the project's no-silent-best-effort convention."

patterns-established:
  - "Path arguments (D-20: str or pathlib.Path) are accepted via direct PyO3 std::path::PathBuf extraction (os.PathLike-aware) rather than a hand-rolled isinstance check -- any other argument type fails extraction with a clear TypeError."

requirements-completed: [PARQ-01, PARQ-02]

coverage:
  - id: D1
    description: "Wave-0 A6 fidelity gate: DataType::Dictionary(dict_is_ordered) and exact non-UTC tz string survive a bare arrow-rs Parquet round-trip via the default embedded ARROW:schema mechanism"
    requirement: PARQ-06
    verification:
      - kind: unit
        ref: "tests/rust/parquet_dictionary_tz_roundtrip.rs#dictionary_and_tz_timestamp_survive_default_parquet_round_trip"
        status: pass
    human_judgment: false
  - id: D2
    description: "A numeric flint.Table round-trips through to_parquet/from_parquet with values and dtypes preserved"
    requirement: PARQ-01
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_roundtrip.py#test_numeric_table_round_trips_through_parquet"
        status: pass
    human_judgment: false
  - id: D3
    description: "A bool-column flint.Table round-trips through to_parquet/from_parquet"
    requirement: PARQ-01
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_roundtrip.py#test_bool_column_round_trips_through_parquet"
        status: pass
    human_judgment: false
  - id: D4
    description: "to_parquet/from_parquet accept a pathlib.Path argument, not just str (D-20)"
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_roundtrip.py#test_pathlib_path_accepted"
        status: pass
    human_judgment: false
  - id: D5
    description: "to_parquet overwrites an existing target file silently, no overwrite= guard flag (D-22)"
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_roundtrip.py#test_to_parquet_overwrites_existing_file_silently"
        status: pass
    human_judgment: false
  - id: D6
    description: "A 0-row Table round-trips through Parquet without raising (empty-table decision, diverges from to_pandas's FlintError::Other)"
    requirement: PARQ-02
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_roundtrip.py#test_empty_table_round_trips"
        status: pass
    human_judgment: false

duration: 40min
completed: 2026-07-23
status: complete
---

# Phase 3 Plan 1: Parquet Walking Skeleton Summary

**End-to-end Parquet round-trip (`Table.to_parquet`/`Table.from_parquet`, snappy-default) landed on a new pyo3-free `flint-core::parquet_io` module, gated by an empirically-passing Wave-0 dictionary/timezone fidelity probe.**

## Performance

- **Duration:** ~40 min
- **Started:** 2026-07-23T20:15:00+08:00 (approx.)
- **Completed:** 2026-07-23T20:52:42+08:00
- **Tasks:** 3
- **Files modified:** 6 (3 created, 3 modified)

## Accomplishments

- Added `parquet = "59.1.0"` to `flint-core`, lockstep-pinned with the existing `arrow = "59.1.0"`.
- Landed the mandatory Wave-0 fidelity gate (RESEARCH.md Assumption A6) as a standalone Rust integration test: proved arrow-rs's default `ArrowWriter`/`ParquetRecordBatchReaderBuilder` behavior (embedded `ARROW:schema` metadata, no explicit schema-hint overrides) preserves `DataType::Dictionary` (with `dict_is_ordered`) and an exact non-UTC IANA tz string through a bare write-then-read. **Gate result: PASSED** — Plans 02-04 can build on this default mechanism for PARQ-06 without an explicit schema-hint workaround.
- Implemented `flint-core::parquet_io::{write_parquet, read_parquet}` (pyo3-free, unit-testable without a Python interpreter): write via crate-default `WriterProperties` (snappy, D-28), read with honest multi-row-group concatenation via `arrow::compute::concat_batches`, zero `.unwrap()`/`.expect()` on any parquet-crate parse `Result`.
- Wired `Table.from_parquet` (`@classmethod`) and `table.to_parquet` (instance method) `#[pymethods]` in `flint-python/src/table.rs`, both accepting `str`/`pathlib.Path` (D-20) via PyO3's `PathBuf` extraction, and both delegating file IO entirely to `flint_core::parquet_io`.
- Authored `tests/python/test_parquet_roundtrip.py` covering numeric, bool, `pathlib.Path`, silent overwrite (D-22), and empty-table (0-row, no exception) cases — all 5 pass.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add parquet dependency + Wave-0 arrow-rs fidelity gate** - `7c32e10` (feat)
2. **Task 2: Failing Python end-to-end round-trip test** - `2bdf89a` (test)
3. **Task 3: Implement parquet_io write/read core + from_parquet/to_parquet #[pymethods]** - `8836e57` (feat)

**Plan metadata:** (final docs commit follows this Summary)

## Files Created/Modified

- `crates/flint-core/Cargo.toml` - Added `parquet = "59.1.0"` dependency + `[[test]]` entry for the Wave-0 gate
- `tests/rust/parquet_dictionary_tz_roundtrip.rs` - Wave-0 A6 fidelity gate (pyo3-free, pure arrow-rs + parquet crate)
- `tests/python/test_parquet_roundtrip.py` - End-to-end Python round-trip tests (numeric, bool, Path, overwrite, empty)
- `crates/flint-core/src/parquet_io.rs` - `write_parquet`/`read_parquet` (pyo3-free core IO logic)
- `crates/flint-core/src/lib.rs` - Added `pub mod parquet_io;`
- `crates/flint-python/src/table.rs` - Added `Table::from_parquet`/`Table::to_parquet` `#[pymethods]`

## Decisions Made

- Wave-0 A6 gate passed empirically (see key-decisions in frontmatter) — no explicit `ARROW:schema` hint mechanism needed for dictionary/tz fidelity at this phase.
- `parquet_io.rs` surfaces errors as `parquet::errors::ParquetError` (not `FlintError`) because `flint-core` cannot depend on `flint-python`'s pyo3-coupled error type; the PyO3 boundary maps it to `FlintError::Other` at the `from_parquet`/`to_parquet` call sites, per the task's own `read_first` guidance (no new `FlintError` variant added this plan).
- Multi-row-group Parquet reads are concatenated into a single `RecordBatch` via `arrow::compute::concat_batches`, matching the project's established honest-concat discipline (CR-01 precedent in `pandas.rs`).
- A genuinely zero-batch `Table` (distinct from a 0-row single-batch `Table`) is rejected on `to_parquet` with a named `FlintError::Other` — this state is unreachable via any current `Table`-construction path (`from_pandas`/`from_parquet`/`from_pytable`/`from_arrow` all always yield exactly one `RecordBatch`), but is guarded explicitly rather than left as an unguarded panic path.

## Deviations from Plan

None — plan executed exactly as written. The `FlintError`-vs-`ParquetError` boundary handling (documented above as a "Decision Made") was anticipated by the task's own `read_first`/`action` text ("wrap parquet-crate read errors via ... FlintError::Other ... do NOT add a new variant in this plan") and is not a deviation from the plan's instructions, just an implementation detail worth recording explicitly.

## Issues Encountered

- `Field::dict_is_ordered()` on the pinned arrow-rs version returns `Option<bool>`, not `bool` — the Wave-0 gate's assertion was written against `bool` initially and failed to compile (`E0600`); fixed by asserting `Some(true)` instead. Caught immediately at compile time, no runtime ambiguity.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- The Wave-0 A6 gate result (PASSED) de-risks Plan 04's dtype-fidelity work: dictionary/categorical and tz-aware timestamp columns can be expected to survive a Flint-internal Parquet round-trip via the default embedded schema mechanism, with no additional schema-hint plumbing required.
- `flint_core::parquet_io::{write_parquet, read_parquet}`'s signatures were kept deliberately Plan-02-extensible (the `None`/crate-default `WriterProperties` argument is the exact seam Plan 02 replaces with a built `WriterProperties` carrying codec + row_group_size).
- WR-01 (nullability bug in `build_field`/`pandas.rs`, flagged in RESEARCH.md and STATE.md Blockers) was deliberately NOT touched in this plan — it is in scope for a later Plan 04 task per `03-01-PLAN.md`'s `files_modified` list, which does not include `pandas.rs`. No regression: the round-trip tests compare via `int64[pyarrow]`/`float64[pyarrow]`/`bool[pyarrow]` `assert_frame_equal`, which does not surface Arrow-level field nullability.
- No blockers for Plan 02 (compression/row-group configuration).

---
*Phase: 03-parquet-io*
*Completed: 2026-07-23*
