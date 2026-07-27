"""Strict zero-copy mode (DIAG-01, D-03).

`from_pandas(df, strict=True)` must SUCCEED on a non-null numeric + ArrowDtype-bool DataFrame
(Success Criterion 2 -- proving strict mode is functional, not a no-op) and REJECT a numpy-backed
bool column with a clear, catchable exception naming the column and dtype. The check is per-column
and pre-flight over the same `plan_column` decision `copy_report()` reads (see
`test_copy_report.py`) -- never a whole-table try/catch (RESEARCH.md Pitfall 2).
"""

import pandas as pd
import pytest

import pydart


def test_strict_mode_succeeds_on_numeric_and_arrow_dtype_bool():
    """Success Criterion 2: strict mode is functional, not a no-op."""
    df = pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "b": pd.array([1.5, 2.5, 3.5], dtype="float64[pyarrow]"),
            "c": pd.array([True, False, True], dtype="bool[pyarrow]"),
        }
    )

    table = pydart.Table.from_pandas(df, strict=True)

    assert table.to_pandas().shape == df.shape


def test_strict_mode_rejects_numpy_backed_bool_column():
    df = pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "flag": pd.Series([True, False, True], dtype=bool),
        }
    )

    with pytest.raises(pydart.ZeroCopyRequiredError) as exc_info:
        pydart.Table.from_pandas(df, strict=True)

    message = str(exc_info.value)
    assert "flag" in message
    assert "bool" in message


def test_strict_mode_rejection_is_catchable_as_pydart_error():
    df = pd.DataFrame({"flag": pd.Series([True, False], dtype=bool)})

    with pytest.raises(pydart.PydartError):
        pydart.Table.from_pandas(df, strict=True)


def test_non_strict_mode_converts_numpy_bool_with_copy_and_does_not_raise():
    df = pd.DataFrame({"flag": pd.Series([True, False, True], dtype=bool)})

    table = pydart.Table.from_pandas(df)  # strict=False (default)
    result = table.to_pandas()

    assert result["flag"].tolist() == df["flag"].tolist()
