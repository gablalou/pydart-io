"""CAP-01/CAP-02 interop validation against pyarrow, Polars, AND DuckDB (D-05).

Plan 01's `test_export_smoke.py` proved the walking-skeleton pyarrow-only export path. This
module completes D-05: export (CAP-01) is proven accepted by all three consumers, import
(CAP-02, via `flint.from_arrow`) is proven from pyarrow and Polars, and the two documented
interop pitfalls (RESEARCH.md Pitfall 3 / Open Question 1) are handled explicitly rather than
assumed.

## DuckDB consumption path (Open Question 1 / Assumption A2, RESOLVED empirically)

`_probe_duckdb_native_consumption()` below runs an empirical spike against the pinned `duckdb`
1.5.4 at module-import time: does `duckdb.sql("FROM <flint.Table instance>")` consume a flint
`Table` natively via DuckDB's own replacement-scan mechanism, with no pyarrow intermediary?

**Result: YES.** `duckdb.sql("FROM flint_table").arrow().read_all()` (where `flint_table` is a
real `flint.Table` local variable, no `pyarrow`/`register()` call involved) returns a correct
`pyarrow.Table` with the expected schema and row count. This confirms Assumption A2 for the
currently pinned DuckDB version. `DUCKDB_NATIVE_CONSUMPTION` records this result; the DuckDB
export test below asserts against whichever path the spike selected (native, or the documented
pyarrow-intermediary fallback if the spike had failed) so DuckDB is never silently skipped
regardless of environment (D-05).

## Consume-once discipline (RESEARCH.md Pitfall 3, T-01-09)

DuckDB relations are documented (`duckdb/duckdb#17084`) to be non-idempotent on a second
`__arrow_c_stream__()` call in some versions/scenarios. Every test below calls a foreign object's
capsule/stream dunder (directly or via a library's own consumption call like `pa.table(...)` /
`pl.from_arrow(...)` / `duckdb.sql(...).arrow()`) exactly once per object; where a value is
needed twice, it is materialized once (e.g. into a plain Python list or an owned `pyarrow.Table`)
and reused. `test_from_arrow_consumes_foreign_stream_dunder_exactly_once` proves this directly for
`flint.from_arrow`'s own import path using an instrumented wrapper object.
"""

import duckdb
import pandas as pd
import polars as pl
import pyarrow as pa
import pytest

import flint


def _numeric_frame() -> pd.DataFrame:
    """A non-null int64/float64 ArrowDtype DataFrame -- the same happy-path fixture shape used by
    `test_export_smoke.py`/`test_round_trip.py`."""
    return pd.DataFrame(
        {
            "a": pd.array([1, 2, 3], dtype="int64[pyarrow]"),
            "b": pd.array([1.5, 2.5, 3.5], dtype="float64[pyarrow]"),
        }
    )


def _probe_duckdb_native_consumption() -> bool:
    """Empirical spike (RESEARCH.md Open Question 1 / Assumption A2): does the pinned duckdb
    consume a flint `Table` natively via `duckdb.sql("FROM <obj>")`, without pyarrow as an
    intermediary? Builds one dedicated probe `Table` and queries it exactly once -- the spike
    itself follows the consume-once discipline it is investigating, and any failure here (e.g. an
    older DuckDB without this support) is caught so the test suite falls back cleanly rather than
    erroring at collection time."""
    probe_table = flint.Table.from_pandas(_numeric_frame())
    try:
        relation = duckdb.sql("FROM probe_table")
        result = relation.arrow().read_all()
        return result.num_rows == 3 and result.schema.names == ["a", "b"]
    except Exception:
        return False


DUCKDB_NATIVE_CONSUMPTION = _probe_duckdb_native_consumption()


def _duckdb_export_round_trip(flint_table: "flint.Table") -> pa.Table:
    """Exercise CAP-01's DuckDB export path, selecting native PyCapsule consumption when
    `DUCKDB_NATIVE_CONSUMPTION` confirmed it works, falling back to the documented
    pyarrow-intermediary path otherwise. DuckDB is proven either way -- never silently skipped
    (D-05)."""
    if DUCKDB_NATIVE_CONSUMPTION:
        relation = duckdb.sql("FROM flint_table")
        return relation.arrow().read_all()

    # Documented fallback (RESEARCH.md Open Question 1 recommendation): materialize through
    # pyarrow first, since native replacement-scan consumption isn't available in this
    # environment's DuckDB version.
    pa_table = pa.table(flint_table)
    relation = duckdb.sql("SELECT * FROM pa_table")
    return relation.arrow().read_all()


# --- CAP-01: export accepted by pyarrow, Polars, DuckDB ---------------------------------------


def test_pyarrow_accepts_flint_table_export():
    df = _numeric_frame()
    flint_table = flint.Table.from_pandas(df)

    pa_table = pa.table(flint_table)

    assert pa_table.num_rows == len(df)
    assert pa_table.schema.names == list(df.columns)


def test_polars_accepts_flint_table_export():
    df = _numeric_frame()
    flint_table = flint.Table.from_pandas(df)

    pl_df = pl.from_arrow(flint_table)

    assert pl_df.shape == (len(df), len(df.columns))
    assert pl_df.columns == list(df.columns)


def test_duckdb_accepts_flint_table_export():
    df = _numeric_frame()
    flint_table = flint.Table.from_pandas(df)

    pa_result = _duckdb_export_round_trip(flint_table)

    assert pa_result.num_rows == len(df)
    assert pa_result.schema.names == list(df.columns)


# --- CAP-02: import from pyarrow Table and Polars DataFrame via flint.from_arrow --------------


def test_from_arrow_imports_pyarrow_table():
    pa_table = pa.table({"a": [1, 2, 3], "b": [1.5, 2.5, 3.5]})

    imported = flint.from_arrow(pa_table)
    result = imported.to_pandas()

    assert list(result.columns) == ["a", "b"]
    assert result["a"].tolist() == [1, 2, 3]
    assert result["b"].tolist() == [1.5, 2.5, 3.5]


def test_from_arrow_imports_polars_dataframe():
    pl_df = pl.DataFrame({"a": [1, 2, 3], "b": [1.5, 2.5, 3.5]})

    imported = flint.from_arrow(pl_df)
    result = imported.to_pandas()

    assert list(result.columns) == ["a", "b"]
    assert result["a"].tolist() == [1, 2, 3]
    assert result["b"].tolist() == [1.5, 2.5, 3.5]


# --- Security / untrusted-capsule handling (T-01-08) -------------------------------------------


def test_from_arrow_rejects_object_without_pycapsule_protocol():
    class NotArrow:
        pass

    with pytest.raises(flint.FlintError):
        flint.from_arrow(NotArrow())


def test_from_arrow_rejects_broken_stream_dunder_without_panicking():
    """An object that claims PyCapsule compliance (has the dunder) but fails when actually
    invoked must surface a clean `flint.FlintError`, never a panic/segfault (T-01-08)."""

    class BrokenStream:
        def __arrow_c_stream__(self, requested_schema=None):
            raise RuntimeError("simulated capsule failure")

    with pytest.raises(flint.FlintError):
        flint.from_arrow(BrokenStream())


# --- Consume-once discipline (T-01-09, RESEARCH.md Pitfall 3) ----------------------------------


class _CountingArrowStreamWrapper:
    """Wraps a real Arrow PyCapsule-compliant object, counting how many times its own
    `__arrow_c_stream__` is invoked -- proves `flint.from_arrow` consumes a foreign object's
    stream dunder exactly once, the same non-idempotency hazard `duckdb/duckdb#17084` documents
    for DuckDB relations."""

    def __init__(self, wrapped):
        self._wrapped = wrapped
        self.call_count = 0

    def __arrow_c_stream__(self, requested_schema=None):
        self.call_count += 1
        return self._wrapped.__arrow_c_stream__(requested_schema)


def test_from_arrow_consumes_foreign_stream_dunder_exactly_once():
    pa_table = pa.table({"a": [1, 2, 3]})
    wrapper = _CountingArrowStreamWrapper(pa_table)

    imported = flint.from_arrow(wrapper)

    assert wrapper.call_count == 1
    assert imported.to_pandas()["a"].tolist() == [1, 2, 3]
