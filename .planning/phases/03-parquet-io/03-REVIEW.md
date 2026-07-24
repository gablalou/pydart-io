---
phase: 03-parquet-io
reviewed: 2026-07-24T00:00:00Z
depth: standard
files_reviewed: 14
files_reviewed_list:
  - crates/flint-core/Cargo.toml
  - crates/flint-core/src/lib.rs
  - crates/flint-core/src/parquet_filter.rs
  - crates/flint-core/src/parquet_io.rs
  - crates/flint-python/src/error.rs
  - crates/flint-python/src/pandas.rs
  - crates/flint-python/src/table.rs
  - tests/python/test_parquet_compression.py
  - tests/python/test_parquet_fidelity.py
  - tests/python/test_parquet_multifile.py
  - tests/python/test_parquet_pushdown.py
  - tests/python/test_parquet_roundtrip.py
  - tests/python/test_wr01_nullability.py
  - tests/rust/parquet_dictionary_tz_roundtrip.rs
  - tests/rust/parquet_row_group_pruning.rs
findings:
  critical: 1
  warning: 4
  info: 1
  total: 6
status: issues_found
---

# Phase 03: Code Review Report

**Reviewed:** 2026-07-24T00:00:00Z
**Depth:** standard
**Files Reviewed:** 14
**Status:** issues_found

## Summary

Reviewed the Parquet IO implementation (`flint-core::parquet_io`/`parquet_filter`, the `flint-python`
`Table::to_parquet`/`from_parquet` boundary, and the Rust/Python test suites for this phase). The
row-group-skip decision logic (`could_match_range`) is careful and its conservative-on-doubt
contract is correctly implemented and unit-tested. However, the row-level `RowFilter` evaluation
path (`evaluate_predicate`) relies on `arrow::compute::cast`'s default "safe" (null-on-overflow)
semantics with no range validation, which can silently produce wrong (under-inclusive) results for
narrower integer column types — a direct violation of this project's own D-26 "RowFilter guarantees
only matching rows, never drops a match" contract. This is the one Critical finding below. The
remaining findings are lower-severity gaps in the multi-file/directory path (an unchecked
`paths[0]` index, non-file entries not filtered out of directory discovery, and per-entry
`read_dir` errors silently swallowed) plus a documented-but-risky schema-equality narrowing and one
missing dtype arm in the row-group-statistics extractor.

## Critical Issues

### CR-01: Row-level filter evaluation can silently drop matching rows for out-of-range literals against narrower integer columns

**File:** `crates/flint-core/src/parquet_io.rs:392-406` (specifically `let casted = cast(&literal, column.data_type())?;` at line 395)

**Issue:** `evaluate_predicate` builds the filter literal in its own native representation
(`Int64`/`Float64`/`Bool`/`Utf8`, see `scalar_value_to_array`) and then calls
`arrow::compute::cast(&literal, column.data_type())` to widen/narrow it to the actual column's
`DataType` before comparing. `arrow::compute::cast` uses `CastOptions::default()`, whose `safe`
field defaults to `true` — meaning a value that does not fit the target type (e.g. casting the
Python-int-derived `Int64(300)` down to `Int8`, whose range is -128..127) produces a **null**
scalar rather than an error.

Once the literal scalar is null, every one of `cmp::eq`/`neq`/`lt`/`lt_eq`/`gt`/`gt_eq` produces a
null (not a `false`) result for every row, and Arrow's row-filter machinery treats null predicate
values as "do not include" (the same as `false`). For `==` this happens to coincide with the
correct answer (no int8 value can equal 300, so "no rows" is correct), but for every other
operator it does **not**:

- `col < 300` on an `int8` column: every value satisfies this (int8 max is 127), so the correct
  result is "all rows." The cast-to-null behavior instead silently returns **zero rows**.
- `col != 300` on an `int8` column: every row satisfies this, correct result is "all rows." The
  cast-to-null behavior instead silently returns **zero rows**.
- `col > -300`, `col <= 300`, `col >= -300`, etc. are symmetric variants of the same problem.

This directly contradicts the module's own documented guarantee (parquet_io.rs's module doc
comment): "even if pruning under- or over-*keeps* row groups... `RowFilter` still guarantees the
returned `Table` contains ONLY matching rows" — the guarantee here is one-directional (never
returns *extra* rows) but says nothing protects against *dropping* rows that should match, and this
is exactly the T-03-05/D-26-forbidden silent-drop direction the rest of the codebase is careful to
avoid (see `could_match_range`'s extensive conservative-on-doubt design in `parquet_filter.rs`,
which this row-level path does not mirror).

This is untested: every pushdown/pruning test in `tests/python/test_parquet_pushdown.py` and
`tests/rust/parquet_row_group_pruning.rs` uses `int64[pyarrow]` columns with filter values that are
always in-range, so this gap does not currently fail CI, but it is reachable by any caller filtering
a narrower column (`int8`/`int16`/`int32`/`uint8`/`uint16`/`uint32`/`uint64` are all
Flint-supported dtypes via `borrow_numpy_numeric_column`/`ArrowDtype`) with an out-of-range Python
int literal — a very plausible real-world mistake (e.g. filtering `("small_int_col", "<", 1000)`
against an `int8[pyarrow]` column).

**Fix:** Validate the literal is representable in the column's target type before casting (or use
`cast_with_options` with `safe: false` and handle the resulting `Err` per-operator), rather than
silently trusting arrow's null-on-overflow cast semantics:

```rust
fn evaluate_predicate(batch: &RecordBatch, op: Op, value: &ScalarValue) -> Result<BooleanArray, ArrowError> {
    let column = batch.column(0);
    let literal = scalar_value_to_array(value);
    let cast_options = arrow::compute::CastOptions { safe: false, ..Default::default() };
    let casted = arrow::compute::cast_with_options(&literal, column.data_type(), &cast_options)
        .map_err(|_| {
            // An out-of-range literal for this column's type: resolve to the operator's correct
            // constant result rather than letting a null literal silently produce "no match" for
            // every row on `!=`/`<`/`<=`/`>`/`>=`.
            ArrowError::CastError(format!(
                "filter literal {value:?} does not fit column type {:?}",
                column.data_type()
            ))
        })?;
    let scalar = arrow::array::Scalar::new(casted);
    match op { /* ... */ }
}
```
At minimum, an explicit error surfaced to the caller (rather than a silently wrong empty/short
result set) satisfies this project's own "never silently drop matching rows" standard; the more
complete fix additionally special-cases each operator's correct constant result when the literal
provably over/underflows the column's range.

## Warnings

### WR-01: `read_parquet_multi` indexes `paths[0]` without a bounds check — panics instead of returning a `Result` on an empty slice

**File:** `crates/flint-core/src/parquet_io.rs:248`

**Issue:** `pub fn read_parquet_multi(paths: &[PathBuf], ...)` begins with `let first_path =
&paths[0];`. This is a `pub` function in a library crate (`flint-core`) whose own doc comment
states "`paths` MUST be non-empty (the PyO3 boundary rejects an empty list/directory before
calling here...)" — i.e. the invariant is enforced entirely by the one current caller
(`flint-python/src/table.rs::resolve_parquet_paths`), not by this function itself. Every other
fallible operation in this module is `?`-propagated into a `Result` per the module's own explicit
"never `.unwrap()`/`.expect()`" discipline; an unchecked slice index is the same class of risk
(an avoidable panic) for a public API whose only current caller happens to validate the
precondition, but which offers no such protection to any other/future caller.

**Fix:**
```rust
pub fn read_parquet_multi(
    paths: &[PathBuf],
    columns: Option<&[String]>,
    filters: &[FilterExpr],
) -> Result<RecordBatch, MultiParquetReadError> {
    let first_path = paths.first().ok_or_else(|| MultiParquetReadError::Read {
        path: String::new(),
        source: ParquetError::General("read_parquet_multi called with an empty path list".to_string()),
    })?;
    ...
}
```

### WR-02: Directory-based Parquet discovery doesn't filter out non-file entries and silently swallows per-entry `read_dir` errors

**File:** `crates/flint-python/src/table.rs:89-98`

**Issue:** Two related gaps in the directory-discovery branch of `resolve_parquet_paths`:

1. `entries.filter_map(|entry| entry.ok())` silently discards any `DirEntry` that fails to read
   (e.g. a permission error, or a removed-mid-iteration entry) with no error surfaced to the
   caller. This directly contradicts this same module's own repeatedly-stated invariant (see the
   doc comments for `resolve_parquet_paths` and `FlintError::ParquetReadError`/
   `InvalidParquetPathArgument`: "never silently skip/include"/"a missing file... never a silent
   skip").
2. The filter only checks `p.extension() == Some("parquet")` — it never checks `p.is_file()`. A
   subdirectory literally named e.g. `archive.parquet` would be included in the discovered file
   list and only fail later, deep inside `read_raw_schema`/`read_parquet`, as an opaque
   `ParquetError` rather than being excluded up front (or reported clearly as "not a file").

**Fix:**
```rust
let mut files: Vec<PathBuf> = entries
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| FlintError::ParquetReadError {
        path: single.display().to_string(),
        reason: e.to_string(),
    })?
    .into_iter()
    .map(|entry| entry.path())
    .filter(|p| p.is_file() && p.extension().and_then(|ext| ext.to_str()) == Some("parquet"))
    .collect();
```

### WR-03: `scalar_from_array` has no `DataType::UInt64` arm — UInt64 columns silently never benefit from row-group pruning, undocumented

**File:** `crates/flint-core/src/parquet_io.rs:427-446` (missing arm; `UInt64Type` also absent from the `use arrow::datatypes::{...}` import list at lines 50-53)

**Issue:** `scalar_from_array` has explicit arms for `Int8`/`Int16`/`Int32`/`Int64`/`UInt8`/
`UInt16`/`UInt32`/`Float32`/`Float64`/`Boolean`, but no `DataType::UInt64` arm — it falls through to
the catch-all `_ => None` ("no stat available", conservatively always-keep). Unlike the `Utf8`/
`LargeUtf8` exclusion, which is explicitly documented above the function (truncated-statistics
rationale), the `UInt64` omission has no comment explaining it, and `uint64` is a genuinely
supported Flint numeric dtype elsewhere (`borrow_numpy_numeric_column`'s `"uint64" => borrow!(u64,
UInt64Type)"` arm in `flint-python/src/pandas.rs`). This is not a correctness bug (the fallback is
safe — it just never prunes), but it is a silent, undocumented gap in an otherwise carefully
enumerated match, and it means `from_parquet(..., filters=[("u64_col", ...)])` never gets the
row-group-skip optimization this feature otherwise provides for every other numeric dtype.

**Fix:** Add the missing arm (and import), or add an explicit doc-comment note excluding it the
way `Utf8`/`LargeUtf8` are excluded, so the omission reads as a decision rather than an oversight:
```rust
DataType::UInt64 => Some(ScalarValue::Int64(array.as_primitive::<UInt64Type>().value(idx) as i64)),
```
(Note: casting `u64 -> i64` can itself overflow for values `> i64::MAX`; if adding this arm, widen
to a lossless representation or explicitly document the same truncation caveat given to Utf8.)

### WR-04: Cross-file schema-equality check omits `Field::dict_is_ordered`, bypassing D-21's `ParquetSchemaMismatch` check for an ordered-vs-unordered mismatch

**File:** `crates/flint-core/src/parquet_io.rs:319-325` (`fields_match`)

**Issue:** `fields_match` compares only `name()`/`data_type()`/`is_nullable()`, and its doc comment
explicitly notes this is narrower than `Field`'s full `PartialEq` "which also compares
dictionary-ordering... metadata a writer might attach", treating that as "incidental." For a
`DataType::Dictionary` column, `ordered` is not incidental metadata — it changes the
comparison/sort semantics of the reconstructed `pd.Categorical` (`ordered=True` enables `<`/`>`
comparisons a `pd.Categorical(ordered=False)` raises `TypeError` for), and `from_pandas`'s own
equivalent classification treats it as schema-significant (`Field::new_dictionary(..)
.with_dict_is_ordered(..)`, see `build_field`'s doc comment in `pandas.rs`). Because `ordered` lives
on `Field` (not `DataType::Dictionary` itself), two dictionary columns with the same name/key/value
types but different `ordered` flags are indistinguishable to `fields_match` and will pass D-21's
"strict cross-file schema match" unchanged — the one purpose-built `ParquetSchemaMismatch` error
this feature exists to raise for exactly this kind of divergence never fires for this specific
mismatch.

What happens downstream once the strict check is bypassed (whether `arrow::compute::concat_batches`
itself independently detects the `ordered` mismatch and errors, or silently concatenates the two
arrays) was not verified against arrow-rs 59.1.0's actual `concat`/`concat_batches` behavior for
this specific case, so this finding is scoped to the verified fact: the intended, precise,
user-actionable `FlintError::ParquetSchemaMismatch` naming the offending column is bypassed for an
`ordered` mismatch, regardless of what (if anything) fails further downstream instead.

**Fix:** Add `ordered` comparison for the `Dictionary` case in `fields_match`:
```rust
fn fields_match(a: &Field, b: &Field) -> bool {
    a.name() == b.name()
        && a.data_type() == b.data_type()
        && a.is_nullable() == b.is_nullable()
        && a.dict_is_ordered() == b.dict_is_ordered()
}
```

## Info

### IN-01: `read_parquet_multi` reads the first file's raw schema twice

**File:** `crates/flint-core/src/parquet_io.rs:249-266`

**Issue:** `first_schema` is read via `read_raw_schema(first_path)` before the loop (line 249-252),
then the loop over `paths` reads `first_path`'s raw schema again on its first iteration (since
`paths[0] == first_path`) purely to compare it against itself. Harmless (always trivially equal)
but a redundant file-open + Parquet-metadata-parse for every multi-file read.

**Fix:** Skip the first element in the loop (`paths.iter().skip(1)`) and reuse `first_schema`
directly, or fold `first_schema`'s assignment into the loop's first iteration instead of computing
it separately beforehand.

---

_Reviewed: 2026-07-24T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
