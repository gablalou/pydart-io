"""D-31/WR-01: `build_field` derives Arrow field nullability from the column's DECLARED source
nullability (threaded from `import_column_via_pandas_stream`'s schema), NOT from
`array.null_count() > 0` (02-REVIEW.md WR-01).

Before this fix, an `int64[pyarrow]` column that happened to contain zero nulls round-tripped as
a `not null` Flint schema field, which broke `pyarrow.concat_tables` against a genuinely-nullable
sibling batch of the same logical column (`ArrowInvalid: Schema ... was different`). Both tests
below are the direct, concrete reproductions from 02-REVIEW.md -- a generic "schema matches"
assertion would NOT catch this (it only breaks on `nullable` specifically, not on any other
schema property).

Recorded, intentional side effect (RESEARCH.md Summary/A5): because pyarrow's
`__arrow_c_stream__` export marks EVERY column `nullable=True`, this fix broadens ALL
stream-imported columns to `nullable=True` uniformly -- the safe/permissive direction (a
nullable-but-dense field never breaks `concat_tables` the way a wrongly-`non-nullable` field
does). No existing test in this suite asserts `nullable=False` on any field (confirmed via
`grep -rn nullable tests/python/` before this file was written), so this broadening does not
regress any existing test.
"""

import pandas as pd
import pyarrow as pa

import flint


def test_nullable_arrow_dtype_zero_nulls_round_trips_as_nullable_field():
    """D-31: an int64[pyarrow] column that is nullable-dtype but contains NO nulls still
    exports a nullable=True Arrow schema field -- NOT a 'not null' field derived from the
    (zero) observed null count."""
    df = pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})
    assert pa.table(df).schema.field("a").nullable is True  # source dtype is nullable

    table = flint.Table.from_pandas(df)
    pa_table = pa.table(table)

    assert pa_table.schema.field("a").nullable is True


def test_concat_tables_across_zero_null_and_nullable_sibling():
    """D-31 (the exact 02-REVIEW.md WR-01 failure): a nullable-but-zero-nulls column and a
    genuinely-nullable sibling batch of the SAME logical column now share a compatible
    (both-nullable) schema, so pyarrow.concat_tables succeeds instead of raising ArrowInvalid
    on a nullability mismatch."""
    df_dense = pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})
    df_with_null = pd.DataFrame({"a": pd.array([4, None, 6], dtype="int64[pyarrow]")})

    t1 = pa.table(flint.Table.from_pandas(df_dense))
    t2 = pa.table(flint.Table.from_pandas(df_with_null))

    # Must not raise pyarrow.lib.ArrowInvalid -- this is the exact reproduction from
    # 02-REVIEW.md WR-01, which failed before the fix with a nullable/not-null schema mismatch.
    combined = pa.concat_tables([t1, t2])

    assert combined.column("a").to_pylist() == [1, 2, 3, 4, None, 6]
