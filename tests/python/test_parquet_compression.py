"""Compression codec + row-group sizing tests (PARQ-02, PARQ-03).

`to_parquet` accepts exactly four D-29 compression codecs (snappy/zstd/gzip/uncompressed) and a
D-30 `row_group_size` row-count parameter. Compression is a WRITE-side-only concern -- a file
written with any codec reads back via `from_parquet` with no codec argument, because the codec is
recorded per-column in the file and applied transparently by the reader. An unsupported codec
string raises `flint.FlintError` naming the offending string, never silently substituting snappy.

Row-group counts are read via `pyarrow.parquet.ParquetFile(path).num_row_groups` since Flint does
not expose row-group counts in its own API this phase (pyarrow is already a dev/test dependency,
see pyproject.toml).
"""

import pandas as pd
import pandas.testing as pdt
import pyarrow.parquet as pq
import pytest

import flint


def _numeric_frame(n: int) -> pd.DataFrame:
    return pd.DataFrame(
        {
            "id": pd.array(range(n), dtype="int64[pyarrow]"),
            "value": pd.array([float(i) * 1.5 for i in range(n)], dtype="float64[pyarrow]"),
        }
    )


@pytest.mark.parametrize("codec", ["snappy", "zstd", "gzip", "uncompressed"])
def test_each_codec_round_trips(tmp_path, codec):
    """Each of the four D-29 codecs writes a file that reads back correctly with NO codec
    argument on the read side -- proving compression is a write-only concern the reader applies
    transparently."""
    df = _numeric_frame(5)
    table = flint.Table.from_pandas(df)

    path = tmp_path / f"{codec}.parquet"
    table.to_parquet(str(path), compression=codec)
    result = flint.Table.from_parquet(str(path))

    pdt.assert_frame_equal(result.to_pandas(), df)


def test_default_codec_is_snappy(tmp_path):
    """D-28: no compression argument defaults to snappy -- verify both correctness AND that the
    file's column-chunk metadata actually reports SNAPPY (not merely that it reads back)."""
    df = _numeric_frame(5)
    table = flint.Table.from_pandas(df)

    path = tmp_path / "default_codec.parquet"
    table.to_parquet(str(path))
    result = flint.Table.from_parquet(str(path))

    pdt.assert_frame_equal(result.to_pandas(), df)

    metadata = pq.ParquetFile(str(path)).metadata
    row_group = metadata.row_group(0)
    for i in range(row_group.num_columns):
        assert row_group.column(i).compression == "SNAPPY"


@pytest.mark.parametrize("bad_codec", ["lz4", "brotli", "gzP", ""])
def test_unknown_codec_raises_flint_error(tmp_path, bad_codec):
    """D-29 rejection: an unsupported codec string raises flint.FlintError naming the offending
    string -- it is NOT silently coerced to snappy or any other default, and no file is written."""
    df = _numeric_frame(3)
    table = flint.Table.from_pandas(df)

    path = tmp_path / "rejected.parquet"

    with pytest.raises(flint.FlintError) as exc_info:
        table.to_parquet(str(path), compression=bad_codec)

    message = str(exc_info.value)
    assert repr(bad_codec) in message or bad_codec in message
    assert not path.exists()


def test_row_group_size_boundary(tmp_path):
    """PARQ-03 adjacency edge: N rows with row_group_size=N/2 produces exactly 2 row groups; M
    rows with row_group_size=M produces exactly 1; M+1 rows with row_group_size=M produces 2
    (boundary is inclusive of the threshold in the first group)."""
    n = 10
    df = _numeric_frame(n)
    table = flint.Table.from_pandas(df)

    path_half = tmp_path / "half.parquet"
    table.to_parquet(str(path_half), row_group_size=n // 2)
    assert pq.ParquetFile(str(path_half)).num_row_groups == 2

    m = 8
    df_m = _numeric_frame(m)
    table_m = flint.Table.from_pandas(df_m)

    path_exact = tmp_path / "exact.parquet"
    table_m.to_parquet(str(path_exact), row_group_size=m)
    assert pq.ParquetFile(str(path_exact)).num_row_groups == 1

    df_m_plus_1 = _numeric_frame(m + 1)
    table_m_plus_1 = flint.Table.from_pandas(df_m_plus_1)

    path_over = tmp_path / "over.parquet"
    table_m_plus_1.to_parquet(str(path_over), row_group_size=m)
    assert pq.ParquetFile(str(path_over)).num_row_groups == 2


def test_row_group_size_default_single_group_small_table(tmp_path):
    """A small Table (< 1,048,576 rows) written with the default row_group_size produces exactly
    1 row group."""
    df = _numeric_frame(100)
    table = flint.Table.from_pandas(df)

    path = tmp_path / "default_row_group.parquet"
    table.to_parquet(str(path))

    assert pq.ParquetFile(str(path)).num_row_groups == 1


def test_row_order_preserved_across_row_groups(tmp_path):
    """PARQ-03 ordering edge: row order is preserved exactly through a write/read round-trip
    regardless of row_group_size -- splitting into multiple row groups does not reorder rows."""
    n = 20
    df = _numeric_frame(n)
    table = flint.Table.from_pandas(df)

    path = tmp_path / "ordering.parquet"
    table.to_parquet(str(path), row_group_size=3)
    assert pq.ParquetFile(str(path)).num_row_groups > 1

    result = flint.Table.from_parquet(str(path))
    pdt.assert_frame_equal(result.to_pandas(), df)


def test_empty_and_single_row_group_counts(tmp_path):
    """PARQ-03 empty/single edge: a 0-row Table writes a valid file that reads back as 0 rows; a
    1-row Table produces exactly 1 row group."""
    empty_df = pd.DataFrame(
        {
            "id": pd.array([], dtype="int64[pyarrow]"),
            "value": pd.array([], dtype="float64[pyarrow]"),
        }
    )
    empty_table = flint.Table.from_pandas(empty_df)
    empty_path = tmp_path / "empty.parquet"
    empty_table.to_parquet(str(empty_path))

    result = flint.Table.from_parquet(str(empty_path))
    assert len(result.to_pandas()) == 0

    single_df = _numeric_frame(1)
    single_table = flint.Table.from_pandas(single_df)
    single_path = tmp_path / "single.parquet"
    single_table.to_parquet(str(single_path))

    assert pq.ParquetFile(str(single_path)).num_row_groups == 1
