---
phase: 01-core-zero-copy-round-trip-interop
reviewed: 2026-07-14T00:00:00Z
depth: standard
files_reviewed: 19
files_reviewed_list:
  - crates/flint-core/Cargo.toml
  - crates/flint-core/src/lib.rs
  - crates/flint-core/src/pandas_plan.rs
  - crates/flint-core/src/table.rs
  - crates/flint-python/Cargo.toml
  - crates/flint-python/src/diagnostics.rs
  - crates/flint-python/src/error.rs
  - crates/flint-python/src/import.rs
  - crates/flint-python/src/lib.rs
  - crates/flint-python/src/pandas.rs
  - crates/flint-python/src/table.rs
  - python/flint/__init__.py
  - tests/python/test_copy_report.py
  - tests/python/test_export_smoke.py
  - tests/python/test_interop.py
  - tests/python/test_round_trip.py
  - tests/python/test_strict_mode.py
  - tests/python/test_zero_copy_pointer.py
  - tests/rust/zero_copy_alloc.rs
findings:
  critical: 1
  warning: 4
  info: 2
  total: 7
status: issues_found
---

# Phase 01: Code Review Report

**Reviewed:** 2026-07-14T00:00:00Z
**Depth:** standard
**Files Reviewed:** 19
**Status:** issues_found

## Summary

Reviewed the `flint-core`/`flint-python` Rust crates, the `flint` Python package shim, and the
Python/Rust test suites for Phase 01 (core zero-copy round-trip + PyCapsule interop). The
zero-copy decision matrix (`plan_column`), the single-error-boundary pattern (`FlintError` ->
`PyErr`), and the strict-mode/`copy_report()` shared-source-of-truth design are all sound and
well-documented, and the pointer-identity / allocation-counting proofs are genuinely
discriminating (they include an explicit negative control).

However, the per-column Arrow-stream import path used for both Arrow-backed columns and the
numpy copy-fallback (`import_column_via_pandas_stream`) only reads the **first** `RecordBatch`
off a potentially multi-batch Arrow C stream and silently discards the rest. This is a genuine
correctness/data-loss risk (BLOCKER) reachable whenever a column's underlying Arrow data has more
than one chunk (the canonical case being `pd.concat()` of `ArrowDtype`-backed frames) — nothing
in the current test suite exercises multi-chunk input, so this is currently undetected. Several
additional edge-case gaps around pandas' nullable ("masked") extension dtypes, an unchecked
length-truncation in `flint_core::from_numpy_buffer`, an unenforced buffer-aliasing hazard, and an
array-offset gap in `buffer_address` (the pointer-identity proof's own instrument) round out the
warnings below.

## Critical Issues

### CR-01: `from_pandas` silently truncates multi-batch Arrow-backed columns to their first chunk

**File:** `crates/flint-python/src/pandas.rs:183-199`
**Issue:**
`import_column_via_pandas_stream` isolates a column into a single-column DataFrame, calls its
`__arrow_c_stream__()`, and imports it via `PyTable::from_arrow_pycapsule`. `PyTable` deliberately
models the Arrow C stream as a `Vec<RecordBatch>` (its `.batches()` accessor returns a slice, not
a single batch) precisely because the Arrow C Data Interface stream protocol permits any number
of batches. This function, however, does:

```rust
let batch = py_table
    .batches()
    .first()
    .ok_or_else(|| FlintError::Other("column stream produced no record batches".to_string()))?;
Ok(batch.column(0).clone())
```

— it reads only the first batch and drops every subsequent one. This function backs BOTH the
genuinely-zero-copy `DtypeBackend::Arrow` path and the `RequiresCopy` numpy-copy-fallback path
(see the `match` in `from_pandas`, lines 155-160), so the bug affects any column whose underlying
pyarrow `ChunkedArray` has more than one chunk — the canonical trigger being
`pd.concat([df1, df2])` of two `ArrowDtype`-backed frames (concatenation does not automatically
rechunk into a single chunk), or any DataFrame built by appending/reading in batches.

This is reachable in two distinct, both-bad ways:
1. **Silent data loss (the dangerous case):** if every Arrow-backed column in the DataFrame has
   the same chunk boundaries (the common `pd.concat` case), every column truncates to the same
   (shorter) row count. `RecordBatch::try_new` sees consistent lengths across all columns and
   **succeeds**, producing a `Table` with fewer rows than the source DataFrame and no error or
   warning at all.
2. **Unattributed generic error:** if columns truncate to different lengths (mixed chunking),
   `RecordBatch::try_new` fails with a generic `arrow::error::ArrowError` (surfaced via
   `FlintError::Arrow`) that does not name the offending column, breaking this project's own
   "always name the offending column" convention (D-03/error.rs doc comment).

Corroborating evidence that this is an oversight rather than an intentional single-batch
invariant: the reverse-direction `Table::to_pandas` (`crates/flint-python/src/table.rs:129`)
correctly does `self.inner.bind(py).get().batches().to_vec()` — forwarding *all* batches. The
per-column import path is the only place in the codebase that truncates to `.first()`.

**Fix:** Concatenate all batches' column-0 arrays instead of taking only the first:

```rust
fn import_column_via_pandas_stream(
    py: Python<'_>,
    df: &Bound<'_, PyAny>,
    column_name: &Bound<'_, PyAny>,
) -> PyResult<ArrayRef> {
    let single_column_selector = PyList::new(py, [column_name])?;
    let single_column_df = df.get_item(single_column_selector)?;
    let capsule: Bound<'_, PyCapsule> = single_column_df
        .call_method0("__arrow_c_stream__")?
        .extract()?;
    let py_table = PyTable::from_arrow_pycapsule(&capsule)?;
    let batches = py_table.batches();
    if batches.is_empty() {
        return Err(FlintError::Other("column stream produced no record batches".to_string()).into());
    }
    let columns: Vec<ArrayRef> = batches.iter().map(|b| b.column(0).clone()).collect();
    let concatenated = arrow::compute::concat(
        &columns.iter().map(|a| a.as_ref()).collect::<Vec<_>>(),
    )
    .map_err(FlintError::from)?;
    Ok(concatenated)
}
```

Note this makes the multi-chunk case an explicit (correctness-preserving) copy rather than a
silent truncation — a multi-chunk column was never a single contiguous buffer to begin with, so
this does not regress the single-chunk zero-copy case `plan_column` already certifies.

## Warnings

### WR-01: Pandas nullable ("masked") extension dtypes crash with a raw `AttributeError` instead of the documented `FlintError`

**File:** `crates/flint-python/src/pandas.rs:84-146`
**Issue:** `classify_dtype` distinguishes `DtypeBackend` solely by `dtype.is_instance(arrow_dtype_type)`
— anything that is not a `pandas.ArrowDtype` is classified `DtypeBackend::Numpy`. Pandas' nullable
extension dtypes (`pd.Int64Dtype()`, `pd.Float64Dtype()`, `pd.BooleanDtype()`, i.e. `"Int64"`,
`"boolean"`, etc.) report `dtype.kind` values of `'i'`/`'f'`/`'b'` respectively (the same as their
numpy counterparts), so they pass the numeric/bool kind check and are classified `Numpy`. But in
`from_pandas`'s contiguity check (lines 136-146):

```rust
DtypeBackend::Numpy => {
    let values = series.getattr("values")?;
    values.getattr("flags")?.getattr("c_contiguous")?.extract::<bool>()?
}
```

`series.values` for a nullable-extension-backed `Series` returns the extension array itself (e.g.
`IntegerArray`), which has no `.flags` attribute. The resulting `AttributeError` propagates as a
raw `PyErr`, bypassing this crate's own `FlintError` boundary entirely (contradicting
`error.rs`'s stated invariant that "every Rust error in this crate flows through `FlintError`").
The caller gets a confusing `AttributeError: 'IntegerArray' object has no attribute 'flags'"
instead of the documented, column/dtype-naming `FlintError::UnsupportedColumn`.

**Fix:** Detect non-numpy, non-Arrow extension arrays explicitly (e.g. check
`hasattr(values, "flags")` or `isinstance(dtype, pd.api.types.is_extension_array_dtype)` before
assuming a plain-numpy `.values`) and reject them via `FlintError::UnsupportedColumn` naming the
column and dtype, consistent with every other out-of-scope dtype.

### WR-02: `flint_core::from_numpy_buffer` silently truncates trailing bytes when `len` is not a multiple of 8

**File:** `crates/flint-core/src/table.rs:49`
**Issue:**
```rust
let scalar_buffer = ScalarBuffer::<i64>::new(buffer, 0, len / std::mem::size_of::<i64>());
```
If `len` is not an exact multiple of `size_of::<i64>()` (8), integer division silently drops the
remainder rather than asserting or returning an error — the function proceeds to construct an
array shorter than the buffer actually is, with no signal to the caller that any truncation
occurred. This is a public (if `unsafe`) API surface re-exported from `flint-core`'s `lib.rs`.

**Fix:** Assert (or return a `Result`) on `len % std::mem::size_of::<i64>() == 0` before dividing,
so malformed input fails loudly instead of silently constructing a truncated array.

### WR-03: Zero-copy numpy borrow does not prevent (or document to Python users) subsequent in-place mutation of the source buffer

**File:** `crates/flint-python/src/pandas.rs:219-288`
**Issue:** `borrow_numpy_numeric_column` wraps a numpy array's live, mutable buffer directly into
an Arrow `Buffer` (whose API contract assumes immutability) and keeps it alive via a `Py<PyArray1<T>>`
owner — but nothing marks the source numpy array read-only or otherwise prevents the caller's own
Python code from mutating it in place after conversion (e.g. `df["a"].values[0] = 999`). Because
the underlying bytes are shared, such a mutation would silently corrupt the "immutable" `Table`'s
data after the fact. This is the standard zero-copy/numpy-borrow tradeoff (pyarrow's own
zero-copy numpy path behaves the same way), so it is not a defect unique to this code, but it is
also not surfaced anywhere in the Python-facing API/docstrings — a user has no way to discover
this hazard short of reading the Rust source.

**Fix:** At minimum, document the aliasing hazard in the Python-facing `Table.from_pandas`
docstring/type stub (e.g. "columns borrowed zero-copy must not be mutated after conversion");
optionally mark the borrowed numpy array `writeable = False` where numpy's API allows it, to fail
fast on an accidental in-place mutation instead of silently corrupting shared data.

### WR-04: `buffer_address` reports the buffer's base pointer, not the array's logical (offset-adjusted) address

**File:** `crates/flint-python/src/table.rs:191-213`
**Issue:**
```rust
let array_data = batch.column(index).to_data();
let address = array_data
    .buffers()
    .first()
    .map(|buffer| buffer.as_ptr() as usize)
    .unwrap_or(0);
```
`ArrayData::buffers()` returns the underlying storage buffer(s) as-is; it does not account for
`ArrayData::offset()`. For an array constructed fresh by `from_pandas` (always offset 0) this is
harmless, but `buffer_address` is also reachable for `Table`s built via `from_arrow` (CAP-02),
which can import an already-sliced foreign array (`offset > 0`) without copying. In that case this
method returns the shared buffer's base address rather than the address of the array's first
logical element — a real discrepancy for the exact instrument (`buffer_address`) this project's
D-06 pointer-identity zero-copy proof depends on to certify "same physical memory," since a
consumer comparing against a slice's own reported data pointer (e.g. pyarrow's `.address` on a
sliced chunk, which IS offset-adjusted) would see a mismatch even though the memory genuinely is
shared.

**Fix:** Adjust for `array_data.offset()` (in elements, scaled by the buffer's logical element
byte-width) when computing the returned address, or explicitly document that `buffer_address`
returns the buffer's base address rather than the array's first-element address, and restrict its
documented use to offset-0 arrays.

## Info

### IN-01: `float16` and other unmatched numeric numpy dtypes fall into a generic `FlintError::Other` instead of `FlintError::UnsupportedColumn`

**File:** `crates/flint-python/src/pandas.rs:268-284`
**Issue:** `classify_dtype` accepts any dtype with `kind` `'f'` as `ArrowKind::Numeric` (including
`float16`), but `borrow_numpy_numeric_column`'s macro-generated match on `dtype_name` has no
`"float16"` arm, so such a column falls into the `other` branch and raises `FlintError::Other`
("unsupported numpy numeric dtype") rather than the more specific, consistently-worded
`FlintError::UnsupportedColumn` used elsewhere for out-of-scope dtypes.
**Fix:** Either add explicit `float16` handling (if in scope) or reject it earlier in
`classify_dtype` with `FlintError::UnsupportedColumn` for consistent error attribution.

### IN-02: `Table.to_pandas(strict=True)` silently accepts and ignores the `strict` argument

**File:** `crates/flint-python/src/table.rs:125-127`
**Issue:** `to_pandas` accepts a `strict` parameter for "API symmetry with `from_pandas`" but does
nothing with it (`let _ = strict;`), since the reverse direction is unconditionally zero-copy. The
Rust doc comment explains this, but nothing surfaces it in a Python-facing docstring/type stub, so
a caller passing `strict=True` expecting it to ever raise would have no signal that it is a no-op.
**Fix:** Document this explicitly in the Python-facing API surface (docstring/`.pyi` stub) rather
than only in the internal Rust doc comment.

---

_Reviewed: 2026-07-14T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
