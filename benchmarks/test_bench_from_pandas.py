"""BENCH-01: throughput benchmark comparing `pydart.Table.from_pandas` against
`pyarrow.Table.from_pandas` across the full six-scenario matrix (D-39: synthetic-only data;
see benchmarks/scenarios.py for each scenario's shape).

Uses the real pydart API -- `pydart.Table.from_pandas` (classmethod, D-19) -- not a module-level
`pydart.from_pandas`, which does not exist (RESEARCH.md's own Pattern 1 code example is stale on
this point; see 04-PATTERNS.md's explicit correction).
"""

import pyarrow as pa
import pytest
from scenarios import SCENARIOS

import pydart


@pytest.mark.parametrize("scenario", SCENARIOS)
def test_from_pandas_pydart(benchmark, scenario, make_df):
    df = make_df(scenario)
    benchmark(pydart.Table.from_pandas, df)


@pytest.mark.parametrize("scenario", SCENARIOS)
def test_from_pandas_pyarrow(benchmark, scenario, make_df):
    df = make_df(scenario)
    benchmark(pa.Table.from_pandas, df)
