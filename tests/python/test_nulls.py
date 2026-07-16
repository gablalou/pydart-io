"""CONV-03: null handling (D-07, D-08, D-09).

Three distinct behaviors, delivered end-to-end by Plan 01's isinstance-first `classify_dtype`
restructure:

- An `ArrowDtype`-backed nullable numeric column (real `pd.NA` nulls) round-trips with null
  positions preserved -- this already worked mechanically via Phase 1's `__arrow_c_stream__`
  import; this file proves it with tests (D-07).
- A pandas masked nullable extension column (`Int64`/`boolean`, capital-letter `pd.NA`-backed) is
  now rejected with an honest `flint.FlintError` naming the column and its concrete dtype type
  name, instead of the raw `AttributeError: '...Array' object has no attribute 'flags'` crash
  fixed by this plan (D-08 / RESEARCH.md Pitfall 1).
- A plain numpy `float64` column containing `NaN` stays on the unchanged Phase 1 zero-copy numeric
  path: `NaN` round-trips as a literal float value, no Arrow null bitmap is introduced, and
  `copy_report()` still reports `zero_copy=True` (D-09).
"""

import math

import pandas as pd
import pandas.testing as pdt
import pytest

import flint


def test_nullable_arrow_dtype_int_round_trips_with_nulls_preserved():
    """D-07: int64[pyarrow] with a real pd.NA null round-trips, null position preserved."""
    df = pd.DataFrame({"a": pd.array([1, None, 3], dtype="int64[pyarrow]")})

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    pdt.assert_frame_equal(result, df)
    assert result["a"][1] is pd.NA
    assert result["a"][0] == 1
    assert result["a"][2] == 3


def test_nullable_arrow_dtype_float_round_trips_with_nulls_preserved():
    """D-07: float64[pyarrow] with a None null round-trips, null position preserved."""
    df = pd.DataFrame({"a": pd.array([1.5, None, 3.5], dtype="float64[pyarrow]")})

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    pdt.assert_frame_equal(result, df)
    assert result["a"][1] is pd.NA
    assert result["a"][0] == 1.5
    assert result["a"][2] == 3.5


def test_masked_int64_extension_dtype_rejected_with_flint_error():
    """D-08 / Pitfall 1: masked (capital-I) Int64 is rejected with an honest flint.FlintError
    naming the column and dtype -- NOT the raw AttributeError this used to crash with."""
    df = pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "masked": pd.array([1, 2, None], dtype="Int64"),
        }
    )

    with pytest.raises(flint.FlintError) as exc_info:
        flint.Table.from_pandas(df)

    assert not isinstance(exc_info.value, AttributeError)
    message = str(exc_info.value)
    assert "masked" in message
    assert "Int64" in message


def test_masked_boolean_extension_dtype_rejected_with_flint_error():
    """D-08 / Pitfall 1: masked `boolean` dtype is rejected the same honest way as `Int64`."""
    df = pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "masked": pd.array([True, False, None], dtype="boolean"),
        }
    )

    with pytest.raises(flint.FlintError) as exc_info:
        flint.Table.from_pandas(df)

    assert not isinstance(exc_info.value, AttributeError)
    message = str(exc_info.value)
    assert "masked" in message
    assert "boolean" in message.lower() or "Boolean" in message


def test_numpy_float64_nan_is_not_treated_as_null():
    """D-09: a plain numpy float64 column's NaN is a literal float value, not an Arrow null --
    it stays on the unchanged Phase 1 zero-copy numeric path (copy_report reports zero_copy=True,
    reason=None)."""
    df = pd.DataFrame({"a": pd.Series([1.0, float("nan"), 3.0], dtype="float64")})

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["a"][0] == 1.0
    assert math.isnan(result["a"][1])
    assert result["a"][2] == 3.0

    report = {status.column: status for status in table.copy_report()}
    assert report["a"].zero_copy is True
    assert report["a"].reason is None
