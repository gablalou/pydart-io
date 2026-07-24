"""D-21: `Table.from_parquet` reads multiple Parquet files (a list of paths, or a directory) as
ONE `Table`, with a strict cross-file schema-match policy (RESEARCH.md Open Question 1) and a
deterministic row order.

- A list of paths concatenates in the GIVEN order (D-21 ordering edge).
- A directory reads only its `.parquet` files, sorted lexicographically by filename (D-21
  ordering edge, deterministic).
- A single-element list behaves identically to passing that one path (D-21 empty/single edge).
- A schema mismatch across files raises `flint.FlintError` (`FlintError::ParquetSchemaMismatch`)
  naming the first file, the mismatched file, and the offending column -- NEVER a silent
  best-effort union/merge (D-21 discretion, strict-match-required v1 default).
- An empty directory (no `.parquet` files) or an empty path list raises `flint.FlintError`
  rather than silently returning an empty `Table` (D-21 empty edge).
"""

import pandas as pd
import pandas.testing as pdt
import pytest

import flint


def _frame(a_values, b_values) -> pd.DataFrame:
    return pd.DataFrame(
        {
            "a": pd.array(a_values, dtype="int64[pyarrow]"),
            "b": pd.array(b_values, dtype="float64[pyarrow]"),
        }
    )


def test_list_of_paths_concatenates_in_given_order(tmp_path):
    df1 = _frame([1, 2], [1.0, 2.0])
    df2 = _frame([3, 4], [3.0, 4.0])

    path1 = tmp_path / "b_second.parquet"
    path2 = tmp_path / "a_first.parquet"
    flint.Table.from_pandas(df1).to_parquet(path1)
    flint.Table.from_pandas(df2).to_parquet(path2)

    # Pass path1 (lexicographically LATER) before path2 -- the concatenation must follow the
    # LIST order given, not directory/lexicographic order (that's the directory-mode behavior
    # tested separately below).
    result = flint.Table.from_parquet([path1, path2]).to_pandas()

    expected = pd.concat([df1, df2], ignore_index=True)
    pdt.assert_frame_equal(result, expected)


def test_directory_reads_only_parquet_files_sorted_lexicographically(tmp_path):
    df_a = _frame([1], [1.0])
    df_b = _frame([2], [2.0])
    df_c = _frame([3], [3.0])

    # Write out of lexicographic order to prove the read sorts, not preserves creation order.
    flint.Table.from_pandas(df_c).to_parquet(tmp_path / "c.parquet")
    flint.Table.from_pandas(df_a).to_parquet(tmp_path / "a.parquet")
    flint.Table.from_pandas(df_b).to_parquet(tmp_path / "b.parquet")
    # A non-.parquet file in the same directory must be silently EXCLUDED, not read/erroring.
    (tmp_path / "notes.txt").write_text("not a parquet file")

    result = flint.Table.from_parquet(tmp_path).to_pandas()

    expected = pd.concat([df_a, df_b, df_c], ignore_index=True)
    pdt.assert_frame_equal(result, expected)


def test_single_element_list_behaves_identically_to_single_path(tmp_path):
    df = _frame([1, 2, 3], [1.5, 2.5, 3.5])
    path = tmp_path / "single.parquet"
    flint.Table.from_pandas(df).to_parquet(path)

    via_single_path = flint.Table.from_parquet(path).to_pandas()
    via_single_element_list = flint.Table.from_parquet([path]).to_pandas()

    pdt.assert_frame_equal(via_single_path, via_single_element_list)
    pdt.assert_frame_equal(via_single_element_list, df)


def test_schema_mismatch_across_files_raises_named_error(tmp_path):
    """D-21: files with disagreeing schemas raise flint.FlintError naming the mismatched file
    and column -- NOT a silently merged/unioned Table."""
    matching = _frame([1, 2], [1.0, 2.0])
    mismatched = pd.DataFrame({"a": pd.array([1, 2], dtype="int64[pyarrow]")})  # missing "b"

    path1 = tmp_path / "matching.parquet"
    path2 = tmp_path / "mismatched.parquet"
    flint.Table.from_pandas(matching).to_parquet(path1)
    flint.Table.from_pandas(mismatched).to_parquet(path2)

    with pytest.raises(flint.FlintError) as exc_info:
        flint.Table.from_parquet([path1, path2])

    message = str(exc_info.value)
    assert str(path1) in message or "matching.parquet" in message
    assert str(path2) in message or "mismatched.parquet" in message
    assert "b" in message


def test_schema_mismatch_different_dtype_raises_named_error(tmp_path):
    """D-21: a same-named column with a different dtype across files is also a schema
    mismatch (not just missing/extra columns)."""
    df1 = pd.DataFrame({"a": pd.array([1, 2], dtype="int64[pyarrow]")})
    df2 = pd.DataFrame({"a": pd.array([1.5, 2.5], dtype="float64[pyarrow]")})

    path1 = tmp_path / "int_a.parquet"
    path2 = tmp_path / "float_a.parquet"
    flint.Table.from_pandas(df1).to_parquet(path1)
    flint.Table.from_pandas(df2).to_parquet(path2)

    with pytest.raises(flint.FlintError) as exc_info:
        flint.Table.from_parquet([path1, path2])

    assert "a" in str(exc_info.value)


def test_empty_directory_raises_error_not_silent_empty_table(tmp_path):
    empty_dir = tmp_path / "empty"
    empty_dir.mkdir()

    with pytest.raises(flint.FlintError):
        flint.Table.from_parquet(empty_dir)


def test_empty_path_list_raises_error_not_silent_empty_table():
    with pytest.raises(flint.FlintError):
        flint.Table.from_parquet([])


def test_nonexistent_path_in_list_raises_named_error(tmp_path):
    """D-21: a nonexistent path passed explicitly as a list element raises
    FlintError::ParquetReadError, which routes through the builtin ValueError (matching the
    existing Arrow/Other wrapped-IO-failure treatment) -- not flint.FlintError, which is
    reserved for caller-input-validation failures (empty list/directory, schema mismatch,
    unsupported codec/operator)."""
    missing = tmp_path / "does_not_exist.parquet"

    with pytest.raises(ValueError) as exc_info:
        flint.Table.from_parquet([missing])

    assert "does_not_exist.parquet" in str(exc_info.value)


def test_multifile_read_combinable_with_columns_and_filters(tmp_path):
    """Multi-file reads compose with the existing columns=/filters= projection/pushdown from
    Plan 03 -- applied consistently per file before concatenation."""
    df1 = _frame([1, 2, 3], [10.0, 20.0, 30.0])
    df2 = _frame([4, 5, 6], [40.0, 50.0, 60.0])
    path1 = tmp_path / "p1.parquet"
    path2 = tmp_path / "p2.parquet"
    flint.Table.from_pandas(df1).to_parquet(path1)
    flint.Table.from_pandas(df2).to_parquet(path2)

    result = flint.Table.from_parquet(
        [path1, path2], columns=["a"], filters=[("a", ">", 2)]
    ).to_pandas()

    assert list(result.columns) == ["a"]
    assert result["a"].tolist() == [3, 4, 5, 6]
