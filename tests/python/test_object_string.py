"""CONV-04: object/string dtype handling (D-10, D-11).

Two distinct string backends, delivered end-to-end by this plan's `ArrowKind::String`
addition to `plan_column`/`classify_dtype`:

- An `ArrowDtype`-backed string column (`pandas.ArrowDtype(pyarrow.string())`, whose
  `str(dtype)` reads `"string[pyarrow]"`) round-trips zero-copy with correct values and
  `None` handling -- it is already Arrow memory (D-10). Note this is NOT the same dtype as
  the bare `dtype="string[pyarrow]"` alias, which pandas resolves to `pandas.StringDtype`
  (a masked extension dtype, rejected the same honest way as masked `Int64`/`boolean`, per
  Plan 01's D-08 rejection) -- these tests construct the genuine `ArrowDtype` explicitly.
- A legacy numpy `object`-dtype column of Python `str` values (with a `None`) round-trips via
  an honest copy, reported as `zero_copy=False` with a reason mentioning the object dtype has
  no Arrow-compatible physical layout (D-10).
- Any object-dtype column containing a non-`str`, non-null value is rejected with a
  Flint-owned `flint.FlintError` naming the column and the offending value's type -- proven
  against dict-valued, all-int, and BOTH orderings of a genuinely mixed-type column, since
  pyarrow's own inference behaves differently (silently, or with a different exception type)
  across all four of these cases (D-11 / RESEARCH.md Pitfall 2).
"""

import pandas as pd
import pandas.testing as pdt
import pyarrow as pa
import pytest

import flint


def test_arrow_dtype_string_round_trips_zero_copy():
    """D-10: a genuine ArrowDtype string column (pandas.ArrowDtype(pyarrow.string())) with a
    None round-trips with correct values/nulls, and copy_report reports zero_copy=True (it is
    already Arrow memory)."""
    df = pd.DataFrame({"a": pd.array(["x", None, "z"], dtype=pd.ArrowDtype(pa.string()))})

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    pdt.assert_frame_equal(result, df)
    assert result["a"][0] == "x"
    assert result["a"][1] is pd.NA
    assert result["a"][2] == "z"

    report = {status.column: status for status in table.copy_report()}
    assert report["a"].zero_copy is True
    assert report["a"].reason is None


def test_numpy_object_string_round_trips_via_copy():
    """D-10: a legacy numpy object-dtype column of Python str (with a None) round-trips via an
    honest copy -- copy_report reports zero_copy=False with a reason mentioning the object
    dtype has no Arrow-compatible physical layout."""
    df = pd.DataFrame({"a": pd.Series(["x", None, "z"], dtype=object)})

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["a"][0] == "x"
    assert result["a"][1] is pd.NA
    assert result["a"][2] == "z"

    report = {status.column: status for status in table.copy_report()}
    assert report["a"].zero_copy is False
    assert report["a"].reason is not None
    assert "object" in report["a"].reason
    assert "copy" in report["a"].reason


def test_object_column_of_ints_rejected():
    """D-11: an all-int object column is rejected -- proves the silent-int64 gap (Pitfall 2)
    is closed, not silently converted."""
    df = pd.DataFrame({"numbers": pd.Series([1, 2, 3], dtype=object)})

    with pytest.raises(flint.FlintError) as exc_info:
        flint.Table.from_pandas(df)

    message = str(exc_info.value)
    assert "numbers" in message
    assert "int" in message


def test_object_column_of_dicts_rejected():
    """D-11: a dict-valued object column is rejected -- proves the silent-struct gap
    (Pitfall 2) is closed, not silently converted."""
    df = pd.DataFrame({"records": pd.Series([{"a": 1}, {"b": 2}], dtype=object)})

    with pytest.raises(flint.FlintError) as exc_info:
        flint.Table.from_pandas(df)

    message = str(exc_info.value)
    assert "records" in message
    assert "dict" in message


def test_object_column_mixed_str_then_int_rejected():
    """D-11: ['a', 123, None] is rejected as a Flint-owned flint.FlintError naming the
    offending int -- not a bare pyarrow ArrowTypeError/ArrowInvalid."""
    df = pd.DataFrame({"mixed": pd.Series(["a", 123, None], dtype=object)})

    with pytest.raises(flint.FlintError) as exc_info:
        flint.Table.from_pandas(df)

    message = str(exc_info.value)
    assert "mixed" in message
    assert "int" in message


def test_object_column_mixed_int_then_str_rejected():
    """D-11: [123, 'a', None] (the OTHER ordering) is ALSO rejected as a Flint-owned
    flint.FlintError naming the offending int -- proves the rejection is order-independent,
    unlike pyarrow's own inference (which raises two DIFFERENT exception types depending on
    ordering, per RESEARCH.md Pitfall 2)."""
    df = pd.DataFrame({"mixed": pd.Series([123, "a", None], dtype=object)})

    with pytest.raises(flint.FlintError) as exc_info:
        flint.Table.from_pandas(df)

    message = str(exc_info.value)
    assert "mixed" in message
    assert "int" in message


def test_empty_object_column_converts_without_error():
    """FLAGGED ASSUMPTION (CONV-04 empty): an empty object-dtype column converts without
    error. Empirically resolved: the resulting Arrow column is typed `null[pyarrow]` (Arrow's
    null type, not string) since there are no values to infer a string type from -- this test
    asserts the no-error, no-rows contract, not a specific Arrow type."""
    df = pd.DataFrame({"a": pd.Series([], dtype=object)})

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    assert len(result) == 0
    assert list(result.columns) == ["a"]


def test_all_none_object_column_converts_without_error():
    """FLAGGED ASSUMPTION (CONV-04 empty): an all-None object-dtype column converts without
    error. Empirically resolved: the resulting Arrow column is typed `null[pyarrow]` (Arrow's
    null type) since every value is null -- this test asserts the no-error, all-null contract,
    not a specific Arrow type."""
    df = pd.DataFrame({"a": pd.Series([None, None, None], dtype=object)})

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    assert len(result) == 3
    assert result["a"].isna().all()
