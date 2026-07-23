---
phase: 02-full-dtype-structural-coverage
reviewed: 2026-07-23T05:53:44Z
depth: standard
files_reviewed: 11
files_reviewed_list:
  - crates/flint-core/Cargo.toml
  - crates/flint-core/src/pandas_plan.rs
  - crates/flint-python/src/error.rs
  - crates/flint-python/src/pandas.rs
  - crates/flint-python/src/table.rs
  - tests/python/test_categorical.py
  - tests/python/test_datetime_timedelta.py
  - tests/python/test_multi_chunk_diagnostics.py
  - tests/python/test_nulls.py
  - tests/python/test_object_string.py
  - tests/rust/concat_generic_arrays.rs
findings:
  critical: 0
  warning: 2
  info: 1
  total: 3
status: issues_found
---

# Phase 02: Code Review Report

**Reviewed:** 2026-07-23T05:53:44Z
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

## Summary

Reviewed the pandas<->Arrow dtype-classification/conversion core (`pandas_plan.rs`, `pandas.rs`,
`table.rs`, `error.rs`), its Cargo manifest, the Rust `concat` probe test, and five Python
integration test files added for this phase's categorical/datetime/timedelta/null/object-string/
multi-chunk coverage.

The full test suite was run (`61 passed`) and several hypotheses were empirically probed
against the built extension (not just read from source) in a live venv with pandas 3.0.3 /
pyarrow 25.0.0:

- The `plan_column` decision matrix, `classify_dtype`'s isinstance-first dispatch ordering
  (Categorical/DatetimeTZDtype before the generic `ExtensionDtype` reject), the ns-resolution
  gating, the multi-chunk `arrow::compute::concat` correction, and the object-dtype content
  validation all behave exactly as documented and as asserted by the test files. No defect found
  in this logic.
- A hypothesized "mutate the original numpy buffer after zero-copy borrow" corruption path was
  probed against several construction patterns; under the pinned pandas 3.0.3 dev dependency,
  pandas' mandatory Copy-on-Write consistently intervened (forcing `.values` read-only or
  defensively copying) before flint ever borrowed the buffer, so it did **not** reproduce in the
  tested environment. It is retained below as a WARNING because the underlying safeguard is
  pandas', not flint's own, and the project's own compatibility table claims support for
  pandas >= 2.2, where CoW is off by default.
- A genuinely reproducible schema-fidelity defect was found and demonstrated end-to-end:
  `build_field` derives Arrow field nullability from the CURRENT batch's observed null count
  rather than from the source dtype's own nullability, silently narrowing a nullable-typed,
  zero-null column to `not null`. This is demonstrated below to break `pyarrow.concat_tables`
  against a schema-compatible external batch.

## Warnings

### WR-01: Field nullability is derived from the batch's null count, not the source dtype's declared nullability, causing an observable schema-fidelity regression

**File:** `crates/flint-python/src/pandas.rs:464-475` (`build_field`), called from `crates/flint-python/src/pandas.rs:444`

**Issue:** Every column's `Field` (both the `DataType::Dictionary` branch and the generic
`other` branch) sets `nullable` via `array.null_count() > 0`. This conflates "this particular
batch happens to contain a null right now" with "this column's type is capable of holding
nulls." A pandas `ArrowDtype`-backed nullable column (e.g. `int64[pyarrow]`) that happens to
have zero nulls in the current batch is round-tripped into a Flint `Table` whose schema
declares that field `not null` — even though the source dtype is nullable and a caller may
reasonably expect the schema to reflect that.

This is directly demonstrable and has a concrete downstream failure mode (verified against the
built extension, pandas 3.0.3 / pyarrow 25.0.0):

```python
import pandas as pd, pyarrow as pa, flint

df = pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})  # nullable dtype, zero nulls
print(pa.table(df).schema.field("a").nullable)                         # True  (source)
print(pa.table(flint.Table.from_pandas(df)).schema.field("a").nullable) # False (flint's Table)
```

Concretely, this breaks schema-union/concat operations against any other genuinely-nullable
batch of the same logical column, even though both sides are `int64` with no incompatible data:

```python
t1 = pa.table(flint.Table.from_pandas(df))                                         # "a: int64 not null"
t2 = pa.table(flint.Table.from_pandas(pd.DataFrame(
    {"a": pd.array([4, None, 6], dtype="int64[pyarrow]")})))                        # "a: int64"
pa.concat_tables([t1, t2])
# pyarrow.lib.ArrowInvalid: Schema at index 1 was different:
# a: int64 not null
# vs
# a: int64
```

This is squarely in this phase's own stated concern (schema-metadata fidelity — the same class
of bug the `dict_is_ordered` fix (D-17/Pitfall 3) exists to prevent) and is silent: nothing
raises, warns, or surfaces in `copy_report()`; the caller only discovers it downstream when a
schema-strict consumer (concat, Parquet append, another Arrow-ecosystem library's schema
equality check) rejects the mismatch.

**Fix:** Derive nullability from the source pandas dtype's actual nullability capability (all
`ArrowDtype`, `DatetimeTZDtype`, and `Categorical` columns are nullable-capable; plain numpy
numeric/bool are not), and thread that through to `build_field` instead of inferring it from
`array.null_count()`:

```rust
// classify_dtype (or from_pandas) should also return whether the source dtype is
// nullable-capable, e.g.:
let source_is_nullable = matches!(dtype_backend, DtypeBackend::Arrow | DtypeBackend::Categorical)
    || matches!(arrow_kind, ArrowKind::Timestamp { .. } | ArrowKind::Duration); // DatetimeTZDtype path

// build_field then uses `source_is_nullable || array.null_count() > 0` instead of
// `array.null_count() > 0` alone, for both the Dictionary and generic arms.
fn build_field(column_name: &str, array: &dyn Array, is_ordered: Option<bool>, nullable: bool) -> Field {
    match array.data_type() {
        DataType::Dictionary(key_type, value_type) => Field::new_dictionary(
            column_name, (**key_type).clone(), (**value_type).clone(), nullable,
        )
        .with_dict_is_ordered(is_ordered.unwrap_or(false)),
        other => Field::new(column_name, other.clone(), nullable),
    }
}
```

---

### WR-02: Zero-copy numpy buffer borrow has no independent immutability guarantee — safety is delegated entirely to the caller's pandas Copy-on-Write behavior

**File:** `crates/flint-python/src/pandas.rs:596-665` (`borrow_numpy_numeric_column`), `crates/flint-python/src/pandas.rs:578-594` (`NumpyBufferOwner`)

**Issue:** `borrow_numpy_numeric_column` wraps a numpy array's raw buffer pointer directly in an
Arrow `Buffer` (`Buffer::from_custom_allocation`) with no data copy, keeping the numpy array
alive via `Py<PyArray1<T>>` (`NumpyBufferOwner`, explicitly `unsafe impl Send + Sync`). The
function's own doc comment (T-01-03) only claims protection against **out-of-bounds reads**
(via `PyReadonlyArray::as_slice()`'s contiguity check) — it does not address **mutation** of the
same buffer through the original numpy array/DataFrame after the borrow, which would silently
corrupt the "immutable" Arrow-side data with no error and no synchronization (a real concern
given `NumpyBufferOwner` is asserted `Sync`, i.e. usable from multiple threads).

I verified empirically (built extension, pandas 3.0.3 pinned in this repo's dev dependencies)
that this specific corruption path does **not** currently reproduce: pandas' Copy-on-Write
(mandatory and non-optional as of pandas >= 3.0) consistently either marks `.values` read-only
or performs a defensive private copy before flint's `plan_column` ever resolves to
`ZeroCopyBorrow`, across direct-assignment mutation, `.values[i] = x` mutation, and
mutate-the-original-2D-array-after-DataFrame-construction. All three attempts left the borrowed
`Table` unchanged.

However, this protection is pandas', not flint's: `dependencies = []` in `pyproject.toml` means
flint declares no pandas runtime floor, and this project's own `.claude/CLAUDE.md` compatibility
table lists `pandas >= 2.2` as a supported baseline for Arrow PyCapsule interop. On pandas < 3.0
(where Copy-on-Write is off by default), `Series.values` for a contiguous numpy-backed column
can return a genuinely writable view sharing memory with the original array, and nothing in this
function's own code prevents `df["col"].values[0] = x` (or any other in-place mutation) from
being written straight through into the Arrow buffer `plan_column` already classified as
zero-copy-safe.

**Fix:** Don't rely on the caller's pandas version/CoW configuration for a safety property this
function's contract needs unconditionally. Force the borrowed array read-only in Rust before
treating its buffer as an immutable Arrow buffer, independent of pandas' own behavior, e.g. via
`PyArrayMethods`'s writeable-flag control on `typed` before constructing `readonly`, so the
zero-copy contract holds deterministically on every supported pandas version rather than only on
the one currently pinned in dev/test.

## Info

### IN-01: `validate_object_column_contents` iterates every value through the Python C API

**File:** `crates/flint-python/src/pandas.rs:547-576`

**Issue:** For every `(DtypeBackend::Numpy, ArrowKind::String)` column, `from_pandas` iterates
the entire column element-by-element via `series.try_iter()` (Python-level iteration, one
`PyAny` extraction/type-check per row) purely for content validation, before the column is
converted at all. This is out of scope as a performance finding per this review's charter, but
worth flagging as a maintainability note: any future contributor extending this validation
(e.g. to also reject `bytes`, or to collect *all* offending rows rather than failing fast on the
first) should be aware this is already a full linear Python-level scan, not a vectorized
pyarrow-side check — the design already accepts that cost once, so incremental additions here
have no further architectural cost to weigh against.

**Fix:** No action required; documenting for future maintainers rather than requesting a change.

---

_Reviewed: 2026-07-23T05:53:44Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
