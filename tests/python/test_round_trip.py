"""End-to-end numeric+bool round-trip tests (CONV-01, CONV-02).

Plan 01 (the walking skeleton) covered non-null int64/float64 `ArrowDtype` columns only. Plan 02
generalizes `from_pandas`/`to_pandas` to the full per-column decision matrix (`plan_column` in
`pydart-core`): `ArrowDtype`-backed numeric/bool columns AND numpy-backed numeric columns (borrowed
zero-copy when contiguous, copied via the pandas/pyarrow stream-export fallback otherwise).
"""

import pandas as pd
import pandas.testing as pdt

import pydart


def _numeric_arrow_dtype_frame() -> pd.DataFrame:
    """A non-null int64/float64 ArrowDtype DataFrame (Phase 1's numeric happy path fixture)."""
    return pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "b": pd.array([1.5, 2.5, 3.5], dtype="float64[pyarrow]"),
        }
    )


def test_from_pandas_to_pandas_round_trip_preserves_values_and_dtypes():
    df = _numeric_arrow_dtype_frame()

    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()

    pdt.assert_frame_equal(result, df)


def test_from_pandas_to_pandas_round_trip_preserves_arrow_dtype_bool():
    """ArrowDtype-backed bool (`"bool[pyarrow]"`) is genuinely zero-copy (Arrow's own bitmap
    layout) and round-trips alongside numeric columns -- Success Criterion 2 / CONV-01 bool
    resolution (RESEARCH.md Pitfall 1, Assumption A1)."""
    df = pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "b": pd.array([True, False, True], dtype="bool[pyarrow]"),
        }
    )

    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()

    pdt.assert_frame_equal(result, df)


def test_from_pandas_to_pandas_round_trip_numpy_numeric_preserves_values():
    """Default numpy-backed int64/float64/int32 columns are borrowed zero-copy (contiguous
    buffer, CONV-01) and round-trip on VALUES. `to_pandas` always reconstructs `ArrowDtype`-backed
    columns (the confirmed zero-copy reverse mechanism -- see `table.rs` doc comment), so a
    numpy-backed input's exact pandas dtype label (e.g. `int64`) is not expected to survive the
    round trip -- only its values are."""
    df = pd.DataFrame(
        {
            "a": pd.Series([1, 2, 3], dtype="int64"),
            "b": pd.Series([1.5, 2.5, 3.5], dtype="float64"),
            "c": pd.Series([10, 20, 30], dtype="int32"),
        }
    )

    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()

    for column in df.columns:
        assert result[column].tolist() == df[column].tolist()


def test_from_pandas_non_contiguous_numpy_column_round_trips_via_copy_fallback():
    """A non-contiguous numpy column (e.g. a stride-2 slice) is NOT zero-copy borrowed --
    `plan_column` classifies it `RequiresCopy` (asserted directly in pydart-core's unit tests) --
    but must still convert correctly via the copy fallback, never crash or misread memory
    (RESEARCH.md Security Domain: contiguity/offset must be checked before any numpy borrow)."""
    base = pd.Series(range(10), dtype="int64")
    sliced = base[::2].reset_index(drop=True)  # stride-2 view: non-contiguous
    df = pd.DataFrame({"a": sliced})

    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["a"].tolist() == df["a"].tolist()


def test_from_pandas_numpy_backed_bool_round_trips_via_copy_fallback():
    """A default numpy-backed bool column requires a bit-packing copy (`plan_column` classifies
    it `RequiresCopy`) but still converts correctly under the default `strict=False` path -- the
    strict-mode REJECTION of this same case (D-03) is covered in `test_strict_mode.py`."""
    df = pd.DataFrame({"a": pd.Series([True, False, True], dtype=bool)})

    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["a"].tolist() == df["a"].tolist()


def test_from_pandas_preserves_all_rows_of_multi_chunk_arrow_backed_column():
    """CR-01 regression: `pd.concat` of two Arrow-backed frames produces a 2-chunk
    `ChunkedArray` (pandas/pyarrow never auto-rechunk on concat). The pre-fix
    `import_column_via_pandas_stream` silently returned only the first `RecordBatch`'s column,
    truncating 6 logical rows down to 3 with no exception. This test builds exactly that
    scenario and asserts the full row count and values survive the round trip."""
    df1 = pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})
    df2 = pd.DataFrame({"a": pd.array([4, 5, 6], dtype="int64[pyarrow]")})
    df = pd.concat([df1, df2], ignore_index=True)

    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()

    assert len(result) == 6
    assert result["a"].tolist() == [1, 2, 3, 4, 5, 6]
