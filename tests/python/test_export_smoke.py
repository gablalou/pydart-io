"""CAP-01 smoke test: export a `pydart.Table` to pyarrow via the Arrow PyCapsule Interface.

This is the Walking Skeleton's single real external-consumer PyCapsule handoff: constructing a
`Table`, handing it to `pyarrow.table(...)` (which consumes `__arrow_c_stream__`), and asserting
the resulting pyarrow Table matches schema and row count with no exception. Full 3-library
interop validation (Polars, DuckDB) is Plan 04.
"""

import pandas as pd
import pyarrow as pa

import pydart


def _numeric_arrow_dtype_frame() -> pd.DataFrame:
    return pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "b": pd.array([1.5, 2.5, 3.5], dtype="float64[pyarrow]"),
        }
    )


def test_pyarrow_table_accepts_pydart_table_via_pycapsule():
    df = _numeric_arrow_dtype_frame()
    table = pydart.Table.from_pandas(df)

    pa_table = pa.table(table)

    assert pa_table.num_rows == len(df)
    assert pa_table.schema.names == list(df.columns)


def test_buffer_address_is_nonzero_for_populated_table():
    df = _numeric_arrow_dtype_frame()
    table = pydart.Table.from_pandas(df)

    assert table.buffer_address(0) != 0


def test_from_pandas_rejects_unsupported_column_with_column_name_in_message():
    df = pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "unsupported": ["x", "y", "z"],
        }
    )

    try:
        pydart.Table.from_pandas(df)
    except Exception as exc:  # noqa: BLE001 - asserting on message content below
        assert "unsupported" in str(exc)
    else:
        raise AssertionError("expected from_pandas to reject an unsupported column")
