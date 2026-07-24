---
phase: 03-parquet-io
fixed_at: 2026-07-24T06:31:20Z
review_path: .planning/phases/03-parquet-io/03-REVIEW.md
iteration: 1
findings_in_scope: 5
fixed: 5
skipped: 0
status: all_fixed
---

# Phase 03: Code Review Fix Report

**Fixed at:** 2026-07-24T06:31:20Z
**Source review:** .planning/phases/03-parquet-io/03-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 5 (fix_scope: critical_warning -> CR-01, WR-01, WR-02, WR-03, WR-04; IN-01 excluded per scope)
- Fixed: 5
- Skipped: 0

## Fixed Issues

### CR-01: Row-level filter evaluation can silently drop matching rows for out-of-range literals against narrower integer columns

**Files modified:** `crates/flint-core/src/parquet_io.rs`
**Commit:** df26820
**Applied fix:** Added an `integer_bounds(&DataType) -> Option<(i128, i128)>` helper covering every
integer Arrow `DataType` (`Int8`..`Int64`, `UInt8`..`UInt64`), and modified `evaluate_predicate` to
check, BEFORE calling `arrow::compute::cast`, whether an `Int64` filter literal falls outside the
target column's integer range. When it does, the function now resolves the operator's provably
correct constant result directly (e.g. `col < 300` against an `Int8` column returns "all non-null
rows match" rather than relying on `cast`'s null-on-overflow producing "zero rows match"). Went
beyond the review's "at minimum, an explicit error" suggestion and implemented the "more complete"
per-operator constant-folding fix the finding described, since it fully satisfies the project's own
D-26 "never silently drop a matching row" contract without forcing every out-of-range filter call
to fail. Null column values continue to never match, matching the pre-existing convention. All 24
`flint-core` unit tests plus the 5 `parquet_row_group_pruning` integration tests and the full Python
suite (141 tests) pass unchanged.

### WR-01: `read_parquet_multi` indexes `paths[0]` without a bounds check

**Files modified:** `crates/flint-core/src/parquet_io.rs`
**Commit:** 4e898a6
**Applied fix:** Replaced the unchecked `&paths[0]` index with
`paths.first().ok_or_else(|| MultiParquetReadError::Read { path: String::new(), source: ParquetError::General(...) })?`,
matching the review's suggested fix exactly. Also updated the function's doc comment to note the
function now defends its own precondition rather than relying solely on the one current caller.

### WR-02: Directory-based Parquet discovery doesn't filter out non-file entries and silently swallows per-entry `read_dir` errors

**Files modified:** `crates/flint-python/src/table.rs`
**Commit:** 07c251e
**Applied fix:** Replaced `entries.filter_map(|entry| entry.ok())` (which silently discarded
per-entry `read_dir` errors) with `entries.collect::<Result<Vec<_>, _>>().map_err(...)?`, surfacing
any entry-read failure as `FlintError::ParquetReadError` naming the directory. Added `p.is_file()`
to the filter predicate alongside the existing `.parquet` extension check, so a subdirectory named
e.g. `archive.parquet` is excluded up front rather than surfacing later as an opaque
`ParquetError` deep inside the read path. Matches the review's suggested fix; confirmed no existing
test in `tests/python/test_parquet_multifile.py` depends on the old silent-skip behavior.

### WR-03: `scalar_from_array` has no `DataType::UInt64` arm

**Files modified:** `crates/flint-core/src/parquet_io.rs`
**Commit:** ee84e42
**Applied fix:** Added `UInt64Type` to the `arrow::datatypes` import list and added a `DataType::UInt64`
arm to `scalar_from_array`. Per the review's own caveat ("casting u64 -> i64 can itself overflow...
widen to a lossless representation or explicitly document the truncation caveat"), chose the
lossless-safety option: `i64::try_from(...).ok().map(ScalarValue::Int64)`, which enables row-group
pruning for `uint64` columns whose values fit in `i64` (the overwhelming majority of real data) and
falls back to the pre-existing conservative "no stat available" `None` for values exceeding
`i64::MAX`, rather than silently truncating/wrapping them. Updated the function's doc comment to
document this new fallback path alongside the existing Utf8/LargeUtf8 rationale.

### WR-04: Cross-file schema-equality check omits `Field::dict_is_ordered`

**Files modified:** `crates/flint-core/src/parquet_io.rs`
**Commit:** 74047ea
**Applied fix:** Added `a.dict_is_ordered() == b.dict_is_ordered()` to `fields_match`'s comparison,
exactly as the review suggested. Updated both `fields_match`'s and `first_schema_mismatch`'s doc
comments to explain why `dict_is_ordered` is schema-significant (affects `pd.Categorical`
comparison semantics) rather than incidental metadata, referencing `from_pandas`'s equivalent
`build_field` classification.

## Skipped Issues

None — all in-scope findings were fixed.

**Out of scope (not attempted, per `fix_scope: critical_warning`):** IN-01 (`read_parquet_multi`
reads the first file's raw schema twice) — an Info-level finding, intentionally left unfixed.

## Verification

- `cargo build --workspace`: clean build, no warnings introduced.
- `cargo test --workspace`: all 24 `flint-core` unit tests + 11 `flint-core` integration tests (across
  `concat_generic_arrays`, `parquet_dictionary_tz_roundtrip`, `parquet_row_group_pruning`,
  `zero_copy_alloc`) pass; `flint-python` unit tests pass (0 tests, as expected — Python-facing
  behavior is covered by the pytest suite below).
- `uv run maturin develop`: builds and installs the extension module cleanly.
- `uv run pytest tests/python -q`: **141 passed**, 0 failed.

All fixes were applied and verified inside an isolated git worktree
(`gsd-reviewfix/03-<pid>`, branched from `master`) per the fixer's isolation protocol, then
fast-forward-merged back onto `master` during cleanup.

---

_Fixed: 2026-07-24T06:31:20Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
