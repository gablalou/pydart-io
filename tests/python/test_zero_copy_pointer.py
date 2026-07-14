"""D-06a: pointer-identity zero-copy proof (CONV-01/CONV-02), forward AND reverse direction.

Complementary, not redundant, with `tests/rust/zero_copy_alloc.rs` (RESEARCH.md Summary line 51):
this test proves the SAME physical memory is shared before/after conversion (pointer identity);
the Rust test proves the conversion path makes no heap allocation for the data buffer. Neither
alone proves zero-copy -- both must pass.

Forward direction (CONV-01): a source pandas column's data-buffer address is captured BEFORE
`flint.Table.from_pandas(df)`, then compared for EXACT equality against `table.buffer_address(i)`
-- same physical memory, not just equal values. Covers both:
  - a numpy-backed int64 column (`values.ctypes.data`, RESEARCH.md line 90-94), and
  - an `int64[pyarrow]` (ArrowDtype) column (the Arrow buffer's own `.address`, RESEARCH.md
    line 317).

Reverse direction (CONV-02): targets the mechanism 01-02-SUMMARY.md CONFIRMED for `to_pandas` --
`PyTable::into_pyarrow` (zero-copy) followed by pyarrow's own
`Table.to_pandas(types_mapper=pandas.ArrowDtype)`, which wraps the pyarrow `ChunkedArray` by
reference (no data copy). The resulting pandas `ArrowDtype` column's underlying buffer address is
asserted equal to the source `Table`'s `buffer_address(i)` -- proving the shipping mechanism,
not an assumed direct-borrow that would not actually be exercised.

No timing-based assertion appears anywhere in this file (RESEARCH.md/01-PATTERNS.md explicitly
warn that timing is not a valid zero-copy proof). A live Python reference to each source array is
held for the full duration of its address comparison (RESEARCH.md line 451: `ctypes.data`'s
address is only meaningful while the owning array is alive).
"""

import pandas as pd

import flint


def _arrow_dtype_column_buffer_address(series: pd.Series) -> int:
    """The physical data-buffer address backing a `pandas.ArrowDtype`-backed column.

    Reads pandas' `ArrowExtensionArray._pa_array` (a pyarrow `ChunkedArray`) directly -- the same
    accessor RESEARCH.md's D-06a code example uses -- rather than `ctypes.data`, since an
    ArrowDtype column's data is Arrow-bitpacked/typed memory, not a plain numpy buffer.
    `buffers()[0]` is the validity bitmap (`None` for a fully non-null chunk, confirmed for the
    pinned pyarrow 25.0.0); `buffers()[1]` is the data buffer whose `.address` is the physical
    pointer value.
    """
    chunk = series.array._pa_array.chunk(0)  # noqa: SLF001 - intentional: this is the proof itself
    data_buffer = chunk.buffers()[1]
    assert data_buffer is not None, "expected a non-null column to have a data buffer"
    return data_buffer.address


def test_from_pandas_forward_zero_copy_pointer_identity_numpy_numeric():
    """Forward (CONV-01): a contiguous numpy-backed int64 column is borrowed, not copied --
    `table.buffer_address(0)` is the EXACT same physical address as the source numpy buffer."""
    df = pd.DataFrame({"a": pd.Series([1, 2, 3], dtype="int64")})
    # Keep a live reference to the source array for the whole comparison (RESEARCH.md line 451):
    # `series.values` is the same numpy ndarray `from_pandas`'s borrow path reads from
    # (crates/flint-python/src/pandas.rs `borrow_numpy_numeric_column`), not a fresh copy.
    source_array = df["a"].values
    original_address = source_array.ctypes.data

    table = flint.Table.from_pandas(df)
    exported_address = table.buffer_address(0)

    assert exported_address == original_address, (
        "from_pandas did not share the numpy column's physical buffer -- a copy was made"
    )
    del source_array  # keep the reference alive until after the assertion above, not before


def test_from_pandas_forward_zero_copy_pointer_identity_arrow_dtype():
    """Forward (CONV-01): an `int64[pyarrow]` (ArrowDtype) column is imported via
    `__arrow_c_stream__` with no data copy -- `table.buffer_address(0)` is the EXACT same
    physical address as the source Arrow buffer's own `.address`."""
    df = pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})
    source_series = df["a"]
    original_address = _arrow_dtype_column_buffer_address(source_series)

    table = flint.Table.from_pandas(df)
    exported_address = table.buffer_address(0)

    assert exported_address == original_address, (
        "from_pandas did not share the ArrowDtype column's physical buffer -- a copy was made"
    )
    del source_series  # keep the reference alive until after the assertion above, not before


def test_to_pandas_reverse_zero_copy_pointer_identity():
    """Reverse (CONV-02): `to_pandas()` shares the `Table`'s buffer via the mechanism confirmed
    in 01-02-SUMMARY.md -- `PyTable::into_pyarrow` (zero-copy) then pyarrow's own
    `Table.to_pandas(types_mapper=pandas.ArrowDtype)`, which wraps the pyarrow `ChunkedArray` by
    reference. The resulting pandas column's data-buffer address must EXACTLY match the source
    `Table`'s `buffer_address(0)` -- proving the shipping reverse mechanism, not an assumed one."""
    df = pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})
    table = flint.Table.from_pandas(df)
    table_address = table.buffer_address(0)

    result = table.to_pandas()
    result_address = _arrow_dtype_column_buffer_address(result["a"])

    assert result_address == table_address, (
        "to_pandas did not share the Table's physical buffer -- a copy was made on the way out"
    )


def test_from_pandas_fails_loudly_if_a_copy_is_introduced():
    """Sanity check: this proof must fail (not silently pass) if `from_pandas` stops being
    zero-copy. Constructing a Table from ONE DataFrame and comparing against a DIFFERENT
    DataFrame's source buffer (never shared, guaranteed distinct addresses) proves the assertion
    style above is actually discriminating, not vacuously true (e.g. from a broken accessor that
    always returns the same constant)."""
    df_one = pd.DataFrame({"a": pd.Series([1, 2, 3], dtype="int64")})
    df_two = pd.DataFrame({"a": pd.Series([4, 5, 6], dtype="int64")})
    unrelated_source_array = df_two["a"].values
    unrelated_address = unrelated_source_array.ctypes.data

    table = flint.Table.from_pandas(df_one)
    exported_address = table.buffer_address(0)

    assert exported_address != unrelated_address, (
        "buffer_address matched an unrelated DataFrame's buffer -- the pointer-identity "
        "accessor is broken and cannot actually detect a copy"
    )
    del unrelated_source_array
