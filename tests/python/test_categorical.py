"""CONV-05: categorical dtype round-trip fidelity (D-17, D-18).

A pandas `Categorical` (ordered or unordered) round-trips through `from_pandas` ->
`to_pandas` as a real `pd.Categorical` (dtype == 'category', with `.cat` accessors),
NOT a `pandas.ArrowDtype` dictionary column, with the exact same category set AND
category order preserved.

This slice fixes two distinct, empirically-verified metadata-loss bugs, both required
together for D-17 to hold end to end:

- Pitfall 3: `from_pandas` previously rebuilt every column's `Field` from
  `array.data_type().clone()`, silently dropping the dictionary `ordered` flag (which lives
  on `Field`, not `DataType`). Pinned here via a direct PyCapsule export with NO `to_pandas`
  call, isolating the fix at its actual root.
- Pitfall 4: `to_pandas`'s previous blanket `types_mapper=pandas.ArrowDtype` reconstructed a
  dictionary column as an `ArrowDtype` dictionary, not a real `Categorical`. Pinned here via
  `dtype == 'category'` and `.cat.ordered`/`.cat.categories`/`.cat.codes` assertions.

D-18 (exact integer code width survives the round trip, not normalized to one fixed width)
is proven with both a small (<=127-category, int8) and a large (>255-category, int16) case.

OQ1 (RESEARCH.md Open Question 1, recorded decision): `to_pandas`'s `strict` parameter stays
the existing documented no-op for categorical columns -- the categorical-reconstruction copy
(pyarrow's own default dictionary reconstruction is not zero-copy for the codes buffer) is an
intentional, documented copy, not surfaced in `copy_report()`.
"""

import pandas as pd
import pyarrow as pa

import flint


def test_ordered_categorical_round_trips_as_real_categorical():
    """D-17: an ordered Categorical round-trips as a real pd.Categorical with the ordered
    flag, category order, and values all preserved -- NOT an ArrowDtype dictionary column."""
    source = pd.Categorical(
        ["b", "a", "c", "a"], categories=["c", "b", "a"], ordered=True
    )
    df = pd.DataFrame({"cat": source})

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["cat"].dtype == "category"
    assert result["cat"].cat.ordered is True
    assert list(result["cat"].cat.categories) == ["c", "b", "a"]
    assert list(result["cat"]) == ["b", "a", "c", "a"]


def test_unordered_categorical_preserves_category_definition_order():
    """D-17: an unordered Categorical with a deliberately non-alphabetical category order
    still preserves that exact definition order (and ordered=False) through the round trip --
    "ordered" and "category order" are independent properties, both must survive."""
    source = pd.Categorical(
        ["z", "a", "m"], categories=["z", "m", "a"], ordered=False
    )
    df = pd.DataFrame({"cat": source})

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["cat"].dtype == "category"
    assert result["cat"].cat.ordered is False
    assert list(result["cat"].cat.categories) == ["z", "m", "a"]
    assert list(result["cat"]) == ["z", "a", "m"]


def test_categorical_code_width_int8_preserved():
    """D-18: a small (<=127-category) categorical's int8 code width survives the round trip
    unchanged, not normalized to a different fixed width."""
    source = pd.Categorical(["a", "b", "c"], categories=["a", "b", "c"], ordered=False)
    df = pd.DataFrame({"cat": source})
    assert df["cat"].cat.codes.dtype == "int8"

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["cat"].cat.codes.dtype == "int8"


def test_categorical_code_width_int16_preserved():
    """D-18: a >255-category categorical's int16 code width survives the round trip
    unchanged -- pandas automatically widens beyond int8 once a column has more than 255
    categories, and Flint must not normalize this back down to int8 or up to int32/int64."""
    categories = [f"c{i}" for i in range(300)]
    source = pd.Categorical(["c0", "c299", "c150"], categories=categories, ordered=False)
    df = pd.DataFrame({"cat": source})
    assert df["cat"].cat.codes.dtype == "int16"

    table = flint.Table.from_pandas(df)
    result = table.to_pandas()

    assert result["cat"].cat.codes.dtype == "int16"
    assert list(result["cat"].cat.categories) == categories
    assert list(result["cat"]) == ["c0", "c299", "c150"]


def test_from_pandas_preserves_ordered_flag_before_to_pandas():
    """D-17 / Pitfall 3 root-cause pin: convert an ordered=True Categorical via
    flint.Table.from_pandas and export DIRECTLY via pa.table(flint_table) (PyCapsule, NO
    to_pandas call at all) -- the exported schema's dictionary field must already report
    ordered=True. This isolates the fix to from_pandas's own Field construction, independent
    of whatever to_pandas's types_mapper does."""
    source = pd.Categorical(["x", "y"], categories=["y", "x"], ordered=True)
    df = pd.DataFrame({"cat": source})

    table = flint.Table.from_pandas(df)
    pa_table = pa.table(table)

    field = pa_table.schema.field("cat")
    assert pa.types.is_dictionary(field.type)
    assert field.type.ordered is True


def test_categorical_reconstruction_copy_is_documented():
    """OQ1: to_pandas(strict=True) does NOT raise for a categorical column -- strict stays a
    documented no-op. The categorical codes-buffer reconstruction (pyarrow's own default
    dictionary reconstruction, which the D-17 fix falls through to) is a known, intentional
    copy per OQ1's recorded decision, not surfaced as a strict-mode violation or in
    copy_report()'s to_pandas-direction diagnostics (which remain a no-op for this direction,
    as documented on Table.to_pandas)."""
    source = pd.Categorical(["a", "b"], ordered=False)
    df = pd.DataFrame({"cat": source})

    table = flint.Table.from_pandas(df)

    # Must not raise: OQ1's recorded decision is that strict stays a no-op for to_pandas even
    # though the categorical reconstruction is an intentional, documented copy.
    result = table.to_pandas(strict=True)
    assert result["cat"].dtype == "category"
