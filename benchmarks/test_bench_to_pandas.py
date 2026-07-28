"""BENCH-01: throughput benchmark comparing `pydart.Table.to_pandas` against
`pyarrow.Table.to_pandas` across the full six-scenario matrix (D-39: synthetic-only data;
see benchmarks/scenarios.py for each scenario's shape).

Mirrors test_bench_from_pandas.py's structure exactly, timing the reverse direction. Both the
pydart table and the pyarrow table are built OUTSIDE the timed `benchmark(...)` call (mirroring
`test_bench_from_pandas.py`'s own `df = make_df(...)` pattern) so only the `to_pandas()` call
itself is measured, not the initial `from_pandas`/`pa.Table.from_pandas` conversion.
"""

import pyarrow as pa
import pytest
from scenarios import SCENARIOS

import pydart


@pytest.mark.parametrize("scenario", SCENARIOS)
def test_to_pandas_pydart(benchmark, scenario, make_df):
    df = make_df(scenario)
    table = pydart.Table.from_pandas(df)
    benchmark(table.to_pandas)


@pytest.mark.parametrize("scenario", SCENARIOS)
def test_to_pandas_pyarrow(benchmark, scenario, make_df):
    df = make_df(scenario)
    pa_table = pa.Table.from_pandas(df)
    benchmark(pa_table.to_pandas)
