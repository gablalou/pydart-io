---
phase: 03-parquet-io
plan: 02
subsystem: io
tags: [rust, pyo3, arrow-rs, parquet, writerproperties, compression, row-group]

# Dependency graph
requires:
  - phase: 03-parquet-io Plan 01
    provides: "flint-core::parquet_io::{write_parquet, read_parquet}, Table.from_parquet/to_parquet #[pymethods], pyo3-free core/PyO3-boundary split, Wave-0 A6 dictionary/tz fidelity gate (PASSED)"
provides:
  - "flint_core::parquet_io::build_writer_properties(codec, row_group_size) -> Result<WriterProperties, ParquetError>: exhaustive four-codec match (snappy/zstd/gzip/uncompressed) + error arm, no silent default"
  - "write_parquet extended to accept a built WriterProperties (replacing Plan 01's None/crate-default), passed as Some(props) to ArrowWriter::try_new"
  - "to_parquet gains compression=\"snappy\" (D-28 default) and row_group_size=1_048_576 (D-30 default) parameters"
  - "FlintError::UnsupportedCodec(String) routed through PyFlintError::new_err -- a named, catchable flint.FlintError naming the offending codec string, never a silent fallback to snappy"
  - "row_group_size=0 guarded at the PyO3 boundary (table.rs) before calling into flint-core, avoiding parquet crate's internal assert!/panic on Some(0)"
affects: ["03-parquet-io Plan 03 (predicate pushdown/projection reads the same WriterProperties-carrying files)", "03-parquet-io Plan 04 (dtype fidelity, multi-file work builds on this writer configuration surface)"]

tech-stack:
  added: []
  patterns:
    - "build_writer_properties lives in flint-core (pyo3-free), NOT flint-python -- keeps parquet_io.rs's documented invariant that it is the ONLY module touching the parquet crate directly (parquet::basic::Compression never leaks into flint-python/table.rs)"
    - "flint-core functions that can only ever fail one way (an exhaustive match with one error arm) let the PyO3 boundary map every Err directly onto the correct named FlintError variant without string-sniffing the underlying error, as long as callers pre-validate any OTHER fallible input (row_group_size==0) before calling in -- keeps the core function's error type honestly single-purpose"

key-files:
  created:
    - tests/python/test_parquet_compression.py
  modified:
    - crates/flint-core/src/parquet_io.rs
    - crates/flint-python/src/error.rs
    - crates/flint-python/src/table.rs

key-decisions:
  - "Resolved a plan/architecture conflict: 03-02-PLAN.md's own text specified build_writer_properties returning Result<WriterProperties, FlintError> inside flint-core, but FlintError lives in flint-python and depends on pyo3 -- flint-core returning it would be a circular dependency, directly contradicting Plan 01's locked decision (flint-core returns parquet::errors::ParquetError; the PyO3 boundary maps to FlintError). Resolved per advisor consultation: build_writer_properties stays in flint-core (keeping parquet::basic::Compression out of flint-python), returns Result<WriterProperties, ParquetError> (unknown codec -> ParquetError::General), and table.rs maps any Err from build_writer_properties directly to FlintError::UnsupportedCodec(compression.to_string()) since codec is that function's SOLE fallible input once row_group_size==0 is guarded separately in table.rs first."
  - "Confirmed the exact row-group-size setter name against the pinned parquet 59.1.0 source (~/.cargo/registry .../parquet-59.1.0/src/file/properties.rs) rather than trusting RESEARCH.md's flagged ambiguity: set_max_row_group_size is #[deprecated(since = \"58.0.0\")]; the correct row-count setter (D-30) is set_max_row_group_row_count(Option<usize>). Used that one."
  - "row_group_size=0 is rejected in table.rs (FlintError::Other) BEFORE calling build_writer_properties, because parquet's set_max_row_group_row_count(Some(0)) internally asserts (panics, would abort the Python interpreter) rather than returning a Result -- this guard was not explicitly specified in the plan but is required by parquet_io.rs's own documented no-panic-on-user-input discipline (Rule 2: missing critical functionality)."

patterns-established:
  - "Core-crate exhaustive-match functions with exactly one error arm are the seam where the PyO3 boundary can construct a specific named FlintError variant directly from the caller's own known input, without needing the core error type to carry PyO3-aware variant information."

requirements-completed: [PARQ-02, PARQ-03]

coverage:
  - id: D1
    description: "All four D-29 compression codecs (snappy/zstd/gzip/uncompressed) write files that read back correctly via from_parquet with no codec argument"
    requirement: PARQ-02
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_compression.py#test_each_codec_round_trips"
        status: pass
    human_judgment: false
  - id: D2
    description: "Default compression (no argument) is snappy, verified both by round-trip correctness and by reading the actual column-chunk compression metadata"
    requirement: PARQ-02
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_compression.py#test_default_codec_is_snappy"
        status: pass
    human_judgment: false
  - id: D3
    description: "An unsupported codec string (lz4/brotli/gzP/empty string) raises flint.FlintError naming the offending string, with no file written and no silent snappy substitution"
    requirement: PARQ-02
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_compression.py#test_unknown_codec_raises_flint_error"
        status: pass
    human_judgment: false
  - id: D4
    description: "row_group_size is interpreted as a row count; boundary (N/2 -> 2 groups, M -> 1, M+1 -> 2), default (small table -> 1 group), and ordering (multi-group round-trip preserves row order) edges all hold"
    requirement: PARQ-03
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_compression.py#test_row_group_size_boundary"
        status: pass
      - kind: e2e
        ref: "tests/python/test_parquet_compression.py#test_row_group_size_default_single_group_small_table"
        status: pass
      - kind: e2e
        ref: "tests/python/test_parquet_compression.py#test_row_order_preserved_across_row_groups"
        status: pass
    human_judgment: false
  - id: D5
    description: "Empty (0-row) and single-row Tables produce correct row-group counts (0 rows read back correctly; 1-row Table produces exactly 1 row group)"
    requirement: PARQ-03
    verification:
      - kind: e2e
        ref: "tests/python/test_parquet_compression.py#test_empty_and_single_row_group_counts"
        status: pass
    human_judgment: false

duration: 10min
completed: 2026-07-23
status: complete
---

# Phase 3 Plan 2: Configurable Parquet Writer (Compression + Row-Group Size) Summary

**`to_parquet` now takes `compression` (snappy/zstd/gzip/uncompressed, default snappy) and `row_group_size` (row count, default 1,048,576) parameters, with an unsupported codec string raising a named `flint.FlintError` instead of silently defaulting.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-07-23T21:00:00+08:00 (approx.)
- **Completed:** 2026-07-23T21:04:35+08:00
- **Tasks:** 2
- **Files modified:** 4 (1 created, 3 modified)

## Accomplishments

- Added `flint_core::parquet_io::build_writer_properties(codec, row_group_size)` — an exhaustive four-arm match over the D-29 codec strings (`"snappy"`, `"zstd"`, `"gzip"`, `"uncompressed"`), mapping to the corresponding `parquet::basic::Compression` variants (including the parameterized `ZSTD(ZstdLevel::default())`/`GZIP(GzipLevel::default())`), with an explicit error arm for anything else — no `.unwrap_or(SNAPPY)` silent default anywhere.
- Confirmed the exact row-group-size setter for the pinned `parquet = "59.1.0"` directly against the crate's own vendored source: `set_max_row_group_size` is `#[deprecated(since = "58.0.0")]`; used the correct row-count setter, `set_max_row_group_row_count(Some(row_group_size))` (D-30).
- Extended `write_parquet` to take a built `WriterProperties` parameter (replacing Plan 01's `None`/crate-default), passed as `Some(properties)` to `ArrowWriter::try_new`.
- Extended `to_parquet`'s PyO3 signature to `(path, compression="snappy", row_group_size=1_048_576)` (D-28/D-30 defaults). Guards `row_group_size == 0` before calling into `flint-core`, since the crate's own setter panics (`assert_ne!`) on `Some(0)` rather than returning a `Result` — this guard prevents a bad Python-side argument from aborting the interpreter.
- Added `FlintError::UnsupportedCodec(String)` to `flint-python/src/error.rs`, routed through `PyFlintError::new_err` (same treatment as `UnsupportedColumn`) — a typo'd codec string is a named, catchable `flint.FlintError`, never a generic builtin exception.
- Authored `tests/python/test_parquet_compression.py` (13 tests): all four codecs round-trip; snappy is confirmed as the actual on-disk default (via `pyarrow.parquet.ParquetFile(...).metadata.row_group(0).column(i).compression`); four bad codec strings each raise `flint.FlintError` naming the string with no file written; row-group boundary/default/ordering/empty/single edges all verified via `pyarrow`'s `num_row_groups` introspection.

## Task Commits

Each task was committed atomically:

1. **Task 1: build_writer_properties (four-codec map + row-count row-group size) + wire into to_parquet + UnsupportedCodec error** - `a8ec291` (feat)
2. **Task 2: Python tests for compression codecs + row-group sizing + codec rejection** - `eaafde4` (test)

**Plan metadata:** (final docs commit follows this Summary)

## Files Created/Modified

- `crates/flint-core/src/parquet_io.rs` - Added `build_writer_properties(codec, row_group_size) -> Result<WriterProperties, ParquetError>`; `write_parquet` now takes a `WriterProperties` parameter instead of building one internally with `None`
- `crates/flint-python/src/error.rs` - Added `FlintError::UnsupportedCodec(String)` variant + `PyFlintError::new_err` routing
- `crates/flint-python/src/table.rs` - `to_parquet` signature extended with `compression`/`row_group_size` params, `row_group_size == 0` guard, calls `build_writer_properties` and maps its `Err` to `FlintError::UnsupportedCodec`
- `tests/python/test_parquet_compression.py` (NEW) - 13 tests covering all four codecs, default-is-snappy verification, codec rejection, and row-group boundary/default/ordering/empty/single edges

## Decisions Made

- **Plan/architecture conflict resolved:** 03-02-PLAN.md's own `<action>` text specified `build_writer_properties(...) -> Result<WriterProperties, FlintError>` living in `flint-core/src/parquet_io.rs`. `FlintError` lives in `flint-python` and depends on `pyo3`; `flint-core` returning it would require a circular crate dependency, directly contradicting Plan 01's already-locked decision that `flint-core` cannot depend on `flint-python` and must surface errors as the underlying library's own `Result` type (`parquet::errors::ParquetError`). Consulted the advisor tool before implementing; resolved as: keep the codec match in `flint-core` (preserving `parquet_io.rs`'s "only module touching the parquet crate" invariant — moving it to `table.rs` would leak `parquet::basic::Compression` into `flint-python`), have it return `Result<WriterProperties, ParquetError>` (unknown codec -> `ParquetError::General(...)`), and have `table.rs` map any `Err` directly to `FlintError::UnsupportedCodec(compression.to_string())` since `table.rs` already holds the original codec string and `build_writer_properties`'s only failure mode (once `row_group_size` is pre-validated) is the codec match.
- Confirmed `set_max_row_group_row_count` (not the deprecated `set_max_row_group_size`, not the byte-based `set_max_row_group_bytes`) by reading the pinned `parquet-59.1.0` crate source directly from the local cargo registry cache, rather than relying on RESEARCH.md's flagged ambiguity (Pitfall 4 / Assumption A1).
- `row_group_size == 0` is rejected in `table.rs` with `FlintError::Other` before any call into `flint-core`, since `set_max_row_group_row_count(Some(0))` panics internally (`assert_ne!`) rather than returning an `Err` — an unguarded `0` would abort the Python interpreter. This keeps `build_writer_properties`'s only fallible input the codec string, so the boundary's `map_err(|_| FlintError::UnsupportedCodec(...))` is never misattributed to a different failure.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed plan-specified circular-dependency error type**
- **Found during:** Task 1 (build_writer_properties implementation)
- **Issue:** The plan's `<action>` text specified `build_writer_properties` in `flint-core` returning `Result<WriterProperties, FlintError>`, but `FlintError` is defined in `flint-python` (depends on `pyo3`) and `flint-core` cannot depend on `flint-python` without creating a circular dependency — this also contradicts Plan 01's own locked architectural decision.
- **Fix:** `build_writer_properties` returns `Result<WriterProperties, ParquetError>` instead (matching Plan 01's established `flint-core` error convention); the PyO3 boundary in `table.rs` maps any `Err` to `FlintError::UnsupportedCodec(compression.to_string())`.
- **Files modified:** `crates/flint-core/src/parquet_io.rs`, `crates/flint-python/src/table.rs`
- **Verification:** `cargo build --workspace` and `cargo test --workspace` both pass; no circular dependency introduced.
- **Committed in:** `a8ec291` (Task 1 commit)

**2. [Rule 2 - Missing Critical] Added row_group_size==0 guard to prevent interpreter abort**
- **Found during:** Task 1 (build_writer_properties implementation)
- **Issue:** The plan did not mention guarding against `row_group_size=0`; the underlying parquet crate's `set_max_row_group_row_count(Some(0))` internally asserts (panics) rather than returning a `Result`, which would abort the Python interpreter on a caller-supplied `0` — violating `parquet_io.rs`'s own documented "never panic on user-influenced input" discipline (T-03-01).
- **Fix:** Added an explicit `row_group_size == 0` check in `table.rs::to_parquet`, returning `FlintError::Other("row_group_size must be greater than 0")` before calling into `flint-core`.
- **Files modified:** `crates/flint-python/src/table.rs`
- **Verification:** Manually reasoned through the parquet crate source (`assert_ne!(value, Some(0), ...)` in `set_max_row_group_row_count`); guard placed before any call reaching that code path.
- **Committed in:** `a8ec291` (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 bug/architecture-conflict fix, 1 missing-critical-functionality guard)
**Impact on plan:** Both fixes were necessary to keep the codebase compiling within its established architecture and to avoid a caller-input-driven interpreter crash. No scope creep — both changes are confined to this plan's own files.

## Issues Encountered

- The plan's own `<action>` text and `03-PATTERNS.md`'s illustrative snippet both specified `build_writer_properties` returning `FlintError` from within `flint-core`, which cannot compile given the crate's no-circular-dependency architecture (Plan 01). Resolved via advisor consultation before implementing (see Decisions Made above) rather than treating this as a Rule 4 architectural-change checkpoint, since the fix restores rather than changes the established architecture.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `to_parquet` now exposes the full PARQ-02/PARQ-03 configuration surface (codec + row-group size); Plan 03 (predicate pushdown/projection) can write test fixtures using any codec/row-group combination without further writer-side changes.
- `build_writer_properties`'s codec-only-fallible design (with `row_group_size` validated by the caller) is a pattern Plan 03/04 can reuse if they add further `flint-core` functions with a single validation point.
- No blockers for Plan 03.

---
*Phase: 03-parquet-io*
*Completed: 2026-07-23*

## Self-Check: PASSED

All created/modified files verified present on disk (`crates/flint-core/src/parquet_io.rs`,
`crates/flint-python/src/error.rs`, `crates/flint-python/src/table.rs`,
`tests/python/test_parquet_compression.py`, this Summary). Both task commit hashes (`a8ec291`,
`eaafde4`) verified present in `git log`. `cargo test --workspace` (12+3+1+2 = all passing) and
`uv run pytest tests/python -q` (79 passed) both green with no regressions.
