"""CONV-06/CONV-07: datetime/tz/timedelta round-trip fidelity and ns-only resolution
gating (D-15, D-16).

The conversion mechanics are already generic (RESEARCH.md Pitfall 0 confirmed datetime/tz/
timedelta round-trip through the existing `__arrow_c_stream__` path) -- this file proves:

- `datetime64[ns]`, `datetime64[ns, tz]`, and `timedelta64[ns]` columns round-trip correctly
  through `from_pandas` -> `to_pandas` (D-15 / CONV-06 / CONV-07).
- A tz-aware timestamp's tz string round-trips EXACTLY as-is (e.g. "America/New_York"), with
  no UTC normalization (D-16).
- Any non-ns-resolution datetime/timedelta column is rejected with a `pydart.PydartError` naming
  the column and its actual resolution, explicitly mentioning pandas 3.0's default-resolution
  change (nanoseconds -> microseconds) and suggesting `.astype('datetime64[ns]')` (D-15 /
  RESEARCH.md Pitfall 5) -- including the realistic `pd.to_datetime(...)`-with-no-explicit-dtype
  failure mode a real pandas-3.0 user actually hits, not just an explicit `dtype='datetime64[us]'`
  construction (Pitfall 5 "Warning signs").
"""

import pandas as pd
import pytest

import pydart


def test_datetime_ns_round_trips():
    """D-15/CONV-06: a datetime64[ns] column round-trips through from_pandas -> to_pandas."""
    df = pd.DataFrame(
        {"a": pd.Series(pd.to_datetime(["2024-01-01", "2024-01-02", "2024-01-03"])).astype(
            "datetime64[ns]"
        )}
    )
    assert df["a"].dtype == "datetime64[ns]"

    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["a"].tolist() == df["a"].tolist()


def test_tz_aware_datetime_ns_round_trips_tz_as_is():
    """D-16: a datetime64[ns, 'America/New_York'] column round-trips values AND the exact tz
    string survives (no UTC normalization) -- to_pandas reconstructs via ArrowDtype (Phase
    1/Plan 01-03's established types_mapper), so the tz is asserted via the reconstructed
    column's underlying pyarrow timestamp type, not a pandas.DatetimeTZDtype."""
    source = pd.to_datetime(["2024-01-01", "2024-01-02"]).tz_localize("America/New_York").as_unit(
        "ns"
    )
    df = pd.DataFrame({"a": pd.Series(source)})
    assert df["a"].dtype.unit == "ns"
    assert str(df["a"].dtype.tz) == "America/New_York"

    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["a"].tolist() == df["a"].tolist()
    assert str(result["a"].dtype.pyarrow_dtype.tz) == "America/New_York"


def test_timedelta_ns_round_trips():
    """D-15/CONV-07: a timedelta64[ns] column round-trips through from_pandas -> to_pandas."""
    df = pd.DataFrame(
        {"a": pd.Series(pd.to_timedelta(["1 days", "2 days", "3 days"])).astype(
            "timedelta64[ns]"
        )}
    )
    assert df["a"].dtype == "timedelta64[ns]"

    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["a"].tolist() == df["a"].tolist()


def test_non_ns_datetime_us_rejected_with_pandas3_message():
    """D-15: an explicit datetime64[us] column is rejected with an actionable error naming the
    column, mentioning "us"/microsecond, pandas 3.0's default-resolution change, and the
    .astype('datetime64[ns]') fix."""
    df = pd.DataFrame({"when": pd.Series(["2024-01-01", "2024-01-02"]).astype("datetime64[us]")})

    with pytest.raises(pydart.PydartError) as exc_info:
        pydart.Table.from_pandas(df)

    message = str(exc_info.value)
    assert "when" in message
    assert "us" in message
    assert "pandas 3.0" in message
    assert "astype" in message
    assert "datetime64[ns]" in message


def test_pd_to_datetime_default_resolution_rejected():
    """Realistic pandas-3.0 failure mode (RESEARCH.md Pitfall 5 "Warning signs"): building a
    datetime column via plain pd.to_datetime(...) with NO explicit dtype/unit now yields
    datetime64[us] on pandas 3.0 (not datetime64[ns] as on earlier pandas versions) -- this is
    the case a real pandas-3.0 user actually hits, and it must be rejected with the same
    actionable message as the explicit-dtype case above, not silently accepted or truncated."""
    df = pd.DataFrame({"when": pd.to_datetime(["2024-01-01", "2024-01-02"])})
    # Pin the premise: no explicit dtype was requested, yet the result is NOT ns-resolution.
    assert df["when"].dtype != "datetime64[ns]"

    with pytest.raises(pydart.PydartError) as exc_info:
        pydart.Table.from_pandas(df)

    message = str(exc_info.value)
    assert "when" in message
    assert "pandas 3.0" in message
    assert "astype" in message
    assert "datetime64[ns]" in message


def test_non_ns_timedelta_us_rejected():
    """D-15: analogous rejection for a timedelta64[us] column -- same actionable message
    shape as the datetime case (naming the column, actual resolution, pandas-3.0 explanation,
    and an .astype fix suggestion, adapted to timedelta64)."""
    df = pd.DataFrame({"delta": pd.to_timedelta(["1 days", "2 days"])})
    assert df["delta"].dtype != "timedelta64[ns]"

    with pytest.raises(pydart.PydartError) as exc_info:
        pydart.Table.from_pandas(df)

    message = str(exc_info.value)
    assert "delta" in message
    assert "us" in message
    assert "pandas 3.0" in message
    assert "astype" in message


def test_datetime_and_timedelta_columns_round_trip_together():
    """Combined-columns sanity check: a datetime64[ns] and a timedelta64[ns] column in the same
    DataFrame both round-trip correctly side by side (no cross-column interference in the
    per-column classify_dtype/plan_column dispatch)."""
    df = pd.DataFrame(
        {
            "when": pd.Series(pd.to_datetime(["2024-01-01", "2024-01-02"])).astype(
                "datetime64[ns]"
            ),
            "delta": pd.Series(pd.to_timedelta(["1 days", "2 days"])).astype("timedelta64[ns]"),
        }
    )

    table = pydart.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["when"].tolist() == df["when"].tolist()
    assert result["delta"].tolist() == df["delta"].tolist()
