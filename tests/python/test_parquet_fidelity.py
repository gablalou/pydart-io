"""PARQ-06: Parquet round-trip preserves logical-type fidelity end-to-end -- categorical/
dictionary encoding (with the `ordered` flag) and exact tz-aware timestamp zone strings -- through
the full `pydart.Table` -> Parquet -> `pydart.Table` path, across the complete Phase 1-2 established
dtype range.

Depends on the WR-01 fix (Task 1 of this plan): PARQ-06 schema fidelity must be validated against
a CORRECT nullability signal, not the pre-fix `array.null_count() > 0` derivation. Plan 01's
Wave-0 A6 gate (`tests/rust/parquet_dictionary_tz_roundtrip.rs`) already proved the underlying
arrow-rs `ARROW:schema` mechanism preserves `DataType::Dictionary`/tz strings at the bare-arrow-rs
level -- this file proves the SAME fidelity holds through Pydart's own `Table`<->pyarrow
construction path (from_pandas -> to_parquet -> from_parquet -> to_pandas), which is a separate
concern from the underlying arrow-rs mechanism. A failure here despite Plan 01's passing A6 gate
would point at the `pydart.Table`<->pyarrow construction path, not the `ARROW:schema` mechanism
itself.

KNOWN GAP (accepted, documented -- see 03-04-SUMMARY.md "Known Gap" section): arrow-rs's
`ArrowWriter`/`DictEncoder` reassigns dictionary keys in first-occurrence-during-encoding order and
drops categories that appear in zero rows. This means a categorical's `.cat.categories` ORDER, and
any UNUSED categories, do NOT survive a Parquet round-trip -- even though `DataType::Dictionary`,
`dict_is_ordered`, and every row's actual value ARE preserved correctly. Confirmed via a pure
arrow-rs/parquet probe independent of Pydart code; no `WriterProperties` knob controls this in
parquet 59.1.0; pyarrow does NOT have this limitation (a genuine arrow-rs-vs-pyarrow divergence,
not a Parquet format limitation). The tests below assert what IS actually guaranteed
(dictionary-ness, ordered flag, per-row values) and deliberately do NOT assert exact
`.cat.categories` order preservation or unused-category retention.
"""

import pandas as pd
import pyarrow as pa

import pydart


def test_ordered_categorical_dictionary_survives_parquet_round_trip(tmp_path):
    """RESEARCH Pitfall 1: an ordered Categorical column written and read back through Parquet
    returns a real pandas Categorical (has .cat accessor) that is STILL DataType::Dictionary (not
    decoded to plain values), with the SAME .cat.ordered (True) and the SAME per-row category
    values. Per the documented known gap (see module docstring), exact .cat.categories ORDER is
    NOT asserted here -- only that the same set of used categories is present."""
    source = pd.Categorical(["b", "a", "c", "a"], categories=["c", "b", "a"], ordered=True)
    df = pd.DataFrame({"cat": source})
    table = pydart.Table.from_pandas(df)

    path = tmp_path / "ordered_cat.parquet"
    table.to_parquet(path)
    result = pydart.Table.from_parquet(path)

    # Assert the ARROW-LEVEL type is still a dictionary (not decoded to plain values) BEFORE
    # even reconstructing pandas -- the direct RESEARCH.md Pitfall 1 assertion shape.
    pa_table = pa.table(result)
    field = pa_table.schema.field("cat")
    assert pa.types.is_dictionary(field.type)
    assert field.type.ordered is True

    result_df = result.to_pandas()
    assert result_df["cat"].dtype == "category"
    assert result_df["cat"].cat.ordered is True
    # Guaranteed: same set of (used) categories and correct per-row values.
    # NOT guaranteed (documented gap): categories ORDER -- see module docstring.
    assert set(result_df["cat"].cat.categories) == set(source.categories)
    assert list(result_df["cat"]) == ["b", "a", "c", "a"]


def test_unordered_categorical_dictionary_survives_parquet_round_trip(tmp_path):
    """RESEARCH Pitfall 1 (ordered=False variant): the ordered=False flag also survives a
    Parquet round trip -- "ordered" is not defaulted to True/False incorrectly either way.
    Category order is not asserted (documented gap, cosmetic for unordered categoricals)."""
    source = pd.Categorical(["z", "a", "m"], categories=["z", "m", "a"], ordered=False)
    df = pd.DataFrame({"cat": source})
    table = pydart.Table.from_pandas(df)

    path = tmp_path / "unordered_cat.parquet"
    table.to_parquet(path)
    result = pydart.Table.from_parquet(path)

    pa_table = pa.table(result)
    field = pa_table.schema.field("cat")
    assert pa.types.is_dictionary(field.type)
    assert field.type.ordered is False

    result_df = result.to_pandas()
    assert result_df["cat"].dtype == "category"
    assert result_df["cat"].cat.ordered is False
    assert set(result_df["cat"].cat.categories) == set(source.categories)
    assert list(result_df["cat"]) == ["z", "a", "m"]


def test_ordered_categorical_category_order_not_guaranteed_known_gap(tmp_path):
    """Documents (does not merely tolerate) the confirmed arrow-rs DictEncoder behavior: for an
    ORDERED categorical, the relative `<` relationship between categories can silently change
    across a Parquet round-trip, because dictionary keys are reassigned in
    first-occurrence-during-encoding row order rather than preserving the original
    `.cat.categories` order. This is a real correctness concern for ordered categoricals (unlike
    the cosmetic case for unordered ones) -- see 03-04-SUMMARY.md Known Gap section. This test
    pins the actual observed behavior so a future arrow-rs upgrade that changes/fixes this is
    caught rather than silently masked."""
    # Original order c < b < a. First-occurrence-in-rows order is b, a, c.
    source = pd.Categorical(["b", "a", "c", "a"], categories=["c", "b", "a"], ordered=True)
    df = pd.DataFrame({"cat": source})
    table = pydart.Table.from_pandas(df)

    path = tmp_path / "ordered_cat_order_gap.parquet"
    table.to_parquet(path)
    result_df = pydart.Table.from_parquet(path).to_pandas()

    # The `ordered` flag itself is preserved...
    assert result_df["cat"].cat.ordered is True
    # ...but the category ORDER (and therefore the `<` relationship) is NOT guaranteed to match
    # the original -- pinned here as the documented, accepted gap rather than asserted as correct.
    assert list(result_df["cat"].cat.categories) != list(source.categories)
    assert list(result_df["cat"].cat.categories) == ["b", "a", "c"]


def test_single_category_dictionary_round_trip(tmp_path):
    """Boundary edge: a single-category dictionary round-trips with fidelity (no reordering
    possible with only one category)."""
    source = pd.Categorical(["only", "only", "only"], categories=["only"], ordered=False)
    df = pd.DataFrame({"cat": source})
    table = pydart.Table.from_pandas(df)

    path = tmp_path / "single_category.parquet"
    table.to_parquet(path)
    result_df = pydart.Table.from_parquet(path).to_pandas()

    assert result_df["cat"].dtype == "category"
    assert list(result_df["cat"].cat.categories) == ["only"]
    assert list(result_df["cat"]) == ["only", "only", "only"]


def test_multi_category_dictionary_round_trip(tmp_path):
    """Boundary edge: a many-category (300) dictionary with only 3 actually-used categories.
    Per the documented known gap, UNUSED categories are dropped on round-trip (arrow-rs's
    DictEncoder only encodes categories that appear in at least one row) -- so the round-tripped
    `.cat.categories` pool contains only the 3 used categories, not the original 300-category
    superset. Dictionary-ness and per-row values remain correct regardless."""
    categories = [f"c{i}" for i in range(300)]
    source = pd.Categorical(["c0", "c299", "c150"], categories=categories, ordered=False)
    df = pd.DataFrame({"cat": source})
    table = pydart.Table.from_pandas(df)

    path = tmp_path / "multi_category.parquet"
    table.to_parquet(path)
    result = pydart.Table.from_parquet(path)

    pa_table = pa.table(result)
    field = pa_table.schema.field("cat")
    assert pa.types.is_dictionary(field.type)

    result_df = result.to_pandas()
    assert result_df["cat"].dtype == "category"
    # Per-row values are correct regardless of internal key/category reassignment.
    assert list(result_df["cat"]) == ["c0", "c299", "c150"]
    # Only the actually-used categories survive (unused-category drop is the documented gap) --
    # assert the used-category SET, not the full 300-category superset.
    assert set(result_df["cat"].cat.categories) == {"c0", "c299", "c150"}


def test_tz_aware_timestamp_exact_zone_survives_parquet_round_trip(tmp_path):
    """RESEARCH Pitfall 2: a tz-aware timestamp column in 'America/New_York' written and read
    back through Parquet returns a column whose tz is EXACTLY 'America/New_York' (not 'UTC', not
    None), with the instant values unchanged."""
    source = (
        pd.to_datetime(["2024-01-01", "2024-06-15", "2024-12-31"])
        .tz_localize("America/New_York")
        .as_unit("ns")
    )
    df = pd.DataFrame({"ts": pd.Series(source)})
    assert str(df["ts"].dtype.tz) == "America/New_York"
    table = pydart.Table.from_pandas(df)

    path = tmp_path / "tz.parquet"
    table.to_parquet(path)
    result_df = pydart.Table.from_parquet(path).to_pandas()

    assert str(result_df["ts"].dtype.pyarrow_dtype.tz) == "America/New_York"
    assert result_df["ts"].tolist() == df["ts"].tolist()


def test_timestamp_boundary_and_ns_precision(tmp_path):
    """Precision/boundary edge: a timestamp at the unix epoch and a far-future instant
    round-trip with the tz preserved and NANOSECOND resolution intact -- no silent truncation to
    us/ms."""
    source = (
        pd.to_datetime(
            [
                "1970-01-01 00:00:00.000000001",  # epoch + 1ns
                "2200-01-01 00:00:00.000000123",  # far-future, sub-microsecond precision
            ]
        )
        .tz_localize("America/New_York")
        .as_unit("ns")
    )
    df = pd.DataFrame({"ts": pd.Series(source)})
    table = pydart.Table.from_pandas(df)

    path = tmp_path / "boundary_ns.parquet"
    table.to_parquet(path)
    result_df = pydart.Table.from_parquet(path).to_pandas()

    assert str(result_df["ts"].dtype.pyarrow_dtype.tz) == "America/New_York"
    assert result_df["ts"].dtype.pyarrow_dtype.unit == "ns"
    assert result_df["ts"].tolist() == df["ts"].tolist()


def test_full_dtype_matrix_parquet_round_trip(tmp_path):
    """Phase-completing check: a single pydart.Table combining the COMPLETE Phase 1-2
    established dtype range -- numeric-with-nulls (CONV-03), string (CONV-04), categorical/
    ordered (CONV-05), tz-aware timestamp (CONV-06), plain non-tz datetime64[ns] (CONV-06), and
    timedelta64[ns]/Duration (CONV-07) -- round-trips through to_parquet/from_parquet. Plain
    datetime and timedelta are load-bearing here: they exercise the same ARROW:schema metadata
    path as tz/dictionary.

    Columns are compared value-by-value (`.tolist()`), not via a single blanket
    `assert_frame_equal`, for two independent reasons:
    - `to_pandas()` always reconstructs columns via pyarrow's `ArrowDtype` types_mapper
      (Phase 1/Plan 01-03's established mechanism -- see test_datetime_timedelta.py), so a
      numpy-backed source dtype (plain `datetime64[ns]`/`timedelta64[ns]`, as this test
      intentionally uses for `plain_dt`/`delta` to exercise that dtype family) never has a
      matching pandas dtype *backend* on the result even when every value round-trips correctly.
      This is established, pre-existing behavior, not a Parquet-fidelity gap.
    - pandas' `CategoricalDtype` equality for an ORDERED categorical requires matching category
      ORDER, and the documented known gap (module docstring) means category order is not
      guaranteed to survive a Parquet round-trip."""
    df = pd.DataFrame(
        {
            "num": pd.array([1, None, 3, 4], dtype="int64[pyarrow]"),
            "str": pd.array(["x", None, "z", "w"], dtype=pd.ArrowDtype(pa.string())),
            "cat": pd.Categorical(
                ["b", "a", "c", "a"], categories=["c", "b", "a"], ordered=True
            ),
            "tz_ts": pd.Series(
                pd.to_datetime(
                    ["2024-01-01", "2024-06-15", "2024-12-31", "2024-03-15"]
                )
                .tz_localize("America/New_York")
                .as_unit("ns")
            ),
            "plain_dt": pd.Series(
                pd.to_datetime(["2024-01-01", "2024-06-15", "2024-12-31", "2024-03-15"])
            ).astype("datetime64[ns]"),
            "delta": pd.to_timedelta(["1 days", "-2 days", "0 days", "365 days"]).astype(
                "timedelta64[ns]"
            ),
        }
    )

    table = pydart.Table.from_pandas(df)
    path = tmp_path / "full_matrix.parquet"
    table.to_parquet(path)
    result_df = pydart.Table.from_parquet(path).to_pandas()

    # Non-categorical columns: per-row value equality (not a blanket assert_frame_equal -- see
    # docstring for why dtype *backend* legitimately differs for numpy-sourced columns).
    for col in ["num", "str", "tz_ts", "plain_dt", "delta"]:
        assert result_df[col].tolist() == df[col].tolist(), f"column {col!r} mismatch"

    # Categorical column: dict-encoding, ordered flag, and per-row values are guaranteed;
    # category order and unused-category retention are the documented known gap (not asserted).
    assert result_df["cat"].dtype == "category"
    assert result_df["cat"].cat.ordered == df["cat"].cat.ordered
    assert set(result_df["cat"].cat.categories) == set(df["cat"].cat.categories)
    assert list(result_df["cat"]) == list(df["cat"])

    # Fidelity-load-bearing assertions beyond plain equality (dictionary-ness + exact tz survive,
    # not just values that happen to compare equal).
    pa_table = pa.table(pydart.Table.from_parquet(path))
    cat_field = pa_table.schema.field("cat")
    assert pa.types.is_dictionary(cat_field.type)
    assert cat_field.type.ordered is True
    assert str(result_df["tz_ts"].dtype.pyarrow_dtype.tz) == "America/New_York"
