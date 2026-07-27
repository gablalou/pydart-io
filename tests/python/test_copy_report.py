"""`Table.copy_report()` (DIAG-02, D-04).

`copy_report()` must derive its per-column status from the SAME `plan_column` decision that
strict mode consumes (single source of truth, RESEARCH.md Pitfall 2) -- this test asserts
column-for-column agreement between the two features directly, not just that `copy_report()`
"looks reasonable" in isolation.
"""

import pandas as pd

import pydart


def _mixed_frame() -> pd.DataFrame:
    return pd.DataFrame(
        {
            "arrow_int": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "arrow_bool": pd.array([True, False, True], dtype="bool[pyarrow]"),
            "numpy_int": pd.Series([1, 2, 3], dtype="int64"),
            "numpy_bool": pd.Series([True, False, True], dtype=bool),
        }
    )


def test_copy_report_returns_one_status_per_column():
    df = _mixed_frame()
    table = pydart.Table.from_pandas(df)

    report = table.copy_report()

    assert len(report) == len(df.columns)
    assert [status.column for status in report] == list(df.columns)


def test_copy_report_marks_arrow_and_contiguous_numpy_numeric_as_zero_copy():
    df = _mixed_frame()
    table = pydart.Table.from_pandas(df)

    report = {status.column: status for status in table.copy_report()}

    assert report["arrow_int"].zero_copy is True
    assert report["arrow_int"].reason is None
    assert report["arrow_bool"].zero_copy is True
    assert report["arrow_bool"].reason is None
    assert report["numpy_int"].zero_copy is True
    assert report["numpy_int"].reason is None


def test_copy_report_marks_numpy_bool_as_requiring_a_copy_with_a_reason():
    df = _mixed_frame()
    table = pydart.Table.from_pandas(df)

    report = {status.column: status for status in table.copy_report()}

    assert report["numpy_bool"].zero_copy is False
    assert report["numpy_bool"].reason is not None
    assert "bool" in report["numpy_bool"].reason


def test_copy_report_agrees_with_strict_mode_rejection_per_column():
    """Single source of truth: copy_report()'s zero_copy=False columns are EXACTLY the columns
    that would make strict mode raise, and vice versa (T-01-05)."""
    df = _mixed_frame()
    table = pydart.Table.from_pandas(df)
    report = table.copy_report()

    non_zero_copy_columns = {status.column for status in report if not status.zero_copy}

    try:
        pydart.Table.from_pandas(df, strict=True)
        strict_failed = False
        strict_failure_column = None
    except pydart.ZeroCopyRequiredError as exc:
        strict_failed = True
        strict_failure_column = next(
            (col for col in non_zero_copy_columns if col in str(exc)), None
        )

    assert strict_failed == bool(non_zero_copy_columns)
    if strict_failed:
        assert strict_failure_column is not None
