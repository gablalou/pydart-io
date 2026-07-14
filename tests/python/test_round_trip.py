"""End-to-end numeric round-trip test (CONV-01 happy path, CONV-02 correctness).

Task 1 of this plan leaves `Table.from_pandas`/`Table.to_pandas` unimplemented on purpose (RED
state) — see 01-01-PLAN.md. Task 2 implements the numeric happy path and this test flips GREEN.
"""

import pandas as pd
import pandas.testing as pdt

import flint


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

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    pdt.assert_frame_equal(result, df)
