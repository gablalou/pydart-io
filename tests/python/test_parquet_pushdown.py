"""Read-side predicate pushdown + column projection tests (PARQ-04, PARQ-05, D-23..D-27).

`from_parquet(columns=[...], filters=[(column, operator, value), ...])` must return ONLY matching
rows (D-26 -- no false positives AND no dropped matches), restricted to the requested columns in
the requested order (D-27/PARQ-05), with `columns`/`filters` independently combinable (D-27: a
filter column need not be in the output projection). Multiple filter tuples combine with AND only
(D-24 -- no OR/nested-list support). An operator string outside the fixed D-25 six-operator set
raises `flint.FlintError` naming the column and operator, never a silently skipped tuple.

`test_operator_coverage_property` is the RESEARCH.md Assumption A3 discriminator: it compares a
filtered `from_parquet` read against an unfiltered-read-then-pandas-filter baseline across all six
operators AND boundary value positions (below the overall min, at the overall min, mid-range within
a non-single-valued row group, at the overall max, above the overall max, and a value equal to a
row group that is entirely single-valued at that value) -- this is what catches `!=` over-pruning
(the D-26-forbidden silent-drop direction) specifically, since `!=` can only skip a row group when
it is provably single-valued and equal to the excluded value.
"""

import operator

import pandas as pd
import pandas.testing as pdt
import pytest

import flint

_OPERATORS = ["==", "!=", "<", "<=", ">", ">="]

# Baseline Python comparison for each D-25 operator string, used to build the
# unfiltered-read-then-pandas-filter baseline the property test compares Flint's pushdown result
# against.
_PY_OPS = {
    "==": operator.eq,
    "!=": operator.ne,
    "<": operator.lt,
    "<=": operator.le,
    ">": operator.gt,
    ">=": operator.ge,
}

# Boundary value positions against the `_pruning_frame` fixture's "v" column (see below): below the
# overall min, at the overall min, mid-range within the first (non-single-valued) row group, equal
# to the middle row group's single constant value, at the overall max, and above the overall max.
_BOUNDARY_VALUES = [-10, 0, 50, 150, 299, 500]


def _abc_frame(n: int) -> pd.DataFrame:
    """Three-column frame for the projection-order test."""
    return pd.DataFrame(
        {
            "a": pd.array(range(n), dtype="int64[pyarrow]"),
            "b": pd.array([i * 2 for i in range(n)], dtype="int64[pyarrow]"),
            "c": pd.array([float(i) * 1.5 for i in range(n)], dtype="float64[pyarrow]"),
        }
    )


def _xy_frame(n: int) -> pd.DataFrame:
    """Two-column integer frame for the single-filter / AND-combination tests. "y" cycles through
    {0, 1, 2} so an AND with an "x" range filter is a genuine, non-trivial intersection."""
    return pd.DataFrame(
        {
            "id": pd.array(range(n), dtype="int64[pyarrow]"),
            "x": pd.array(range(n), dtype="int64[pyarrow]"),
            "y": pd.array([i % 3 for i in range(n)], dtype="int64[pyarrow]"),
        }
    )


def _ab_frame(n: int) -> pd.DataFrame:
    """Two-column integer frame for the projection+filter-combinability test (filter column "b" is
    deliberately excluded from the output projection in that test)."""
    return pd.DataFrame(
        {
            "a": pd.array(range(n), dtype="int64[pyarrow]"),
            "b": pd.array(range(n), dtype="int64[pyarrow]"),
        }
    )


def _pruning_frame(n: int = 300) -> pd.DataFrame:
    """300-row, `row_group_size=100` fixture producing three row groups on column "v":
    - group 0 (rows 0-99):   values 0..99      (varied, non-single-valued, min=0, max=99)
    - group 1 (rows 100-199): value 150 repeated (single-valued -- exercises the `!=` skip rule)
    - group 2 (rows 200-299): values 200..299   (varied, non-single-valued, min=200, max=299)

    "id" is a distinct monotonic column carried alongside "v" purely so the property test's
    `assert_frame_equal` also implicitly checks that row order is preserved through a
    filtered/pruned read.
    """
    values = list(range(100)) + [150] * 100 + list(range(200, 300))
    assert len(values) == n
    return pd.DataFrame(
        {
            "id": pd.array(range(n), dtype="int64[pyarrow]"),
            "v": pd.array(values, dtype="int64[pyarrow]"),
        }
    )


def test_projection_returns_subset_in_order(tmp_path):
    """PARQ-05: columns=["c","a"] returns exactly columns c,a in that order (not schema order)."""
    df = _abc_frame(20)
    table = flint.Table.from_pandas(df)
    path = tmp_path / "abc.parquet"
    table.to_parquet(str(path))

    result = flint.Table.from_parquet(str(path), columns=["c", "a"])
    result_df = result.to_pandas()

    assert list(result_df.columns) == ["c", "a"]
    pdt.assert_frame_equal(result_df.reset_index(drop=True), df[["c", "a"]].reset_index(drop=True))


def test_single_filter_returns_only_matching_rows(tmp_path):
    """D-26: from_parquet(filters=[("x",">",5)]) returns every row with x>5 AND exactly that many
    rows -- no false positives, no dropped matches."""
    n = 300
    df = _xy_frame(n)
    table = flint.Table.from_pandas(df)
    path = tmp_path / "xy.parquet"
    table.to_parquet(str(path), row_group_size=100)

    result = flint.Table.from_parquet(str(path), filters=[("x", ">", 5)]).to_pandas()
    expected = df[df["x"] > 5].reset_index(drop=True)

    assert (result["x"] > 5).all()
    assert len(result) == len(expected)
    pdt.assert_frame_equal(result.reset_index(drop=True), expected)


def test_and_combination(tmp_path):
    """D-24: multiple filter tuples combine with AND only."""
    n = 300
    df = _xy_frame(n)
    table = flint.Table.from_pandas(df)
    path = tmp_path / "xy_and.parquet"
    table.to_parquet(str(path), row_group_size=100)

    result = flint.Table.from_parquet(
        str(path), filters=[("x", ">", 5), ("y", "==", 1)]
    ).to_pandas()
    expected = df[(df["x"] > 5) & (df["y"] == 1)].reset_index(drop=True)

    assert (result["x"] > 5).all()
    assert (result["y"] == 1).all()
    assert len(result) == len(expected)
    pdt.assert_frame_equal(result.reset_index(drop=True), expected)


def test_projection_and_filter_combinable_filter_col_not_projected(tmp_path):
    """D-27: columns=["a"], filters=[("b","<",10)] -- "b" drives filtering but is NOT in the
    output; the reader decodes the filter-eval column separately from the output columns."""
    n = 50
    df = _ab_frame(n)
    table = flint.Table.from_pandas(df)
    path = tmp_path / "ab.parquet"
    table.to_parquet(str(path), row_group_size=10)

    result = flint.Table.from_parquet(
        str(path), columns=["a"], filters=[("b", "<", 10)]
    ).to_pandas()
    expected = df[df["b"] < 10][["a"]].reset_index(drop=True)

    assert list(result.columns) == ["a"]
    assert len(result) == len(expected)
    pdt.assert_frame_equal(result.reset_index(drop=True), expected)


@pytest.mark.parametrize("value", _BOUNDARY_VALUES)
@pytest.mark.parametrize("op", _OPERATORS)
def test_operator_coverage_property(tmp_path, op, value):
    """RESEARCH.md Assumption A3 discriminator: for every (operator, boundary value) pair, compare
    Flint's pushdown read against an unfiltered-read-then-pandas-filter baseline. Includes
    `min < value < max` (value=50, within group 0) and `min == max == value` (value=150, group 1 is
    entirely single-valued) for `!=` specifically -- the two cases that distinguish correct
    conservative pruning from over-pruning."""
    df = _pruning_frame()
    table = flint.Table.from_pandas(df)
    path = tmp_path / "pruning.parquet"
    table.to_parquet(str(path), row_group_size=100)

    result = flint.Table.from_parquet(str(path), filters=[("v", op, value)]).to_pandas()
    expected = df[_PY_OPS[op](df["v"], value)].reset_index(drop=True)

    pdt.assert_frame_equal(result.reset_index(drop=True), expected)


def test_unknown_operator_raises(tmp_path):
    """D-25: an operator string outside the fixed six-operator set raises flint.FlintError naming
    the column and operator -- never a silently skipped/ignored filter tuple."""
    df = _xy_frame(20)
    table = flint.Table.from_pandas(df)
    path = tmp_path / "unknown_op.parquet"
    table.to_parquet(str(path))

    with pytest.raises(flint.FlintError) as exc_info:
        flint.Table.from_parquet(str(path), filters=[("x", "in", [1, 2])])

    message = str(exc_info.value)
    assert "in" in message
    assert "x" in message


def test_filter_on_stats_less_or_empty(tmp_path):
    """PARQ-04 edge: an empty (0-row) file returns 0 rows under any filter, and a filter matching
    nothing on a non-empty file returns a 0-row Table (not an error)."""
    empty_df = pd.DataFrame({"x": pd.array([], dtype="int64[pyarrow]")})
    empty_table = flint.Table.from_pandas(empty_df)
    empty_path = tmp_path / "empty.parquet"
    empty_table.to_parquet(str(empty_path))

    empty_result = flint.Table.from_parquet(str(empty_path), filters=[("x", ">", 5)]).to_pandas()
    assert len(empty_result) == 0

    df = _xy_frame(50)
    table = flint.Table.from_pandas(df)
    path = tmp_path / "no_match.parquet"
    table.to_parquet(str(path), row_group_size=10)

    no_match_result = flint.Table.from_parquet(
        str(path), filters=[("x", ">", 1_000_000)]
    ).to_pandas()
    assert len(no_match_result) == 0


def test_idempotent_double_read(tmp_path):
    """Running the same from_parquet(filters=..., columns=...) twice on the same file yields
    identical Tables (a pure read, no shared/mutated state)."""
    df = _xy_frame(100)
    table = flint.Table.from_pandas(df)
    path = tmp_path / "idempotent.parquet"
    table.to_parquet(str(path), row_group_size=25)

    first = flint.Table.from_parquet(
        str(path), columns=["y", "x"], filters=[("x", ">=", 10)]
    ).to_pandas()
    second = flint.Table.from_parquet(
        str(path), columns=["y", "x"], filters=[("x", ">=", 10)]
    ).to_pandas()

    pdt.assert_frame_equal(first, second)
