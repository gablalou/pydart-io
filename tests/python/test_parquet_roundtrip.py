"""End-to-end Parquet round-trip tests (PARQ-01, basic PARQ-02/D-28).

These target `Table.from_parquet`/`table.to_parquet` (D-19: classmethod/instance method, same
`from_X`/`to_X` shape as `from_pandas`/`to_pandas`; no module-level `pydart.read_parquet`/
`write_parquet`). Until Task 3 lands the `#[pymethods]`, every test here fails with an
AttributeError -- that failure is expected and recorded as the MVP failing-test-first step; Task 3
turns them green.
"""

import pandas as pd
import pandas.testing as pdt

import pydart


def _numeric_arrow_dtype_frame() -> pd.DataFrame:
    return pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "b": pd.array([1.5, 2.5, 3.5], dtype="float64[pyarrow]"),
        }
    )


def test_numeric_table_round_trips_through_parquet(tmp_path):
    df = _numeric_arrow_dtype_frame()
    table = pydart.Table.from_pandas(df)

    path = tmp_path / "t.parquet"
    table.to_parquet(str(path))
    result = pydart.Table.from_parquet(str(path))

    pdt.assert_frame_equal(result.to_pandas(), df)


def test_bool_column_round_trips_through_parquet(tmp_path):
    df = pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "b": pd.array([True, False, True], dtype="bool[pyarrow]"),
        }
    )
    table = pydart.Table.from_pandas(df)

    path = tmp_path / "bool.parquet"
    table.to_parquet(str(path))
    result = pydart.Table.from_parquet(str(path))

    pdt.assert_frame_equal(result.to_pandas(), df)


def test_pathlib_path_accepted(tmp_path):
    """D-20: from_parquet/to_parquet accept a pathlib.Path (not just str)."""
    df = _numeric_arrow_dtype_frame()
    table = pydart.Table.from_pandas(df)

    path = tmp_path / "pathlib.parquet"
    table.to_parquet(path)
    result = pydart.Table.from_parquet(path)

    pdt.assert_frame_equal(result.to_pandas(), df)


def test_to_parquet_overwrites_existing_file_silently(tmp_path):
    """D-22: to_parquet overwrites an existing target file with no overwrite= guard flag --
    a from_parquet read afterward returns the SECOND table's data."""
    path = tmp_path / "overwrite.parquet"

    first_df = _numeric_arrow_dtype_frame()
    pydart.Table.from_pandas(first_df).to_parquet(str(path))

    second_df = pd.DataFrame(
        {
            "a": pd.array([100, 200], dtype="int64[pyarrow]"),
            "b": pd.array([9.9, 8.8], dtype="float64[pyarrow]"),
        }
    )
    pydart.Table.from_pandas(second_df).to_parquet(str(path))

    result = pydart.Table.from_parquet(str(path))
    pdt.assert_frame_equal(result.to_pandas(), second_df)


def test_empty_table_round_trips(tmp_path):
    """Empty-table decision (must_haves): a 0-row Table round-trips through Parquet without
    raising -- diverges from to_pandas's empty-table PydartError::Other, because an empty Parquet
    file carrying just the schema is a valid, readable artifact."""
    empty_df = pd.DataFrame(
        {
            "a": pd.array([], dtype="int64[pyarrow]"),
            "b": pd.array([], dtype="float64[pyarrow]"),
        }
    )
    table = pydart.Table.from_pandas(empty_df)

    path = tmp_path / "empty.parquet"
    table.to_parquet(str(path))
    result = pydart.Table.from_parquet(str(path))

    result_df = result.to_pandas()
    assert len(result_df) == 0
    pdt.assert_frame_equal(result_df, empty_df)
