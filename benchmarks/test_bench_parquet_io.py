"""BENCH-01: throughput benchmark comparing `pydart` Parquet write/read against pyarrow's
equivalents, across the full six-scenario matrix (D-39: synthetic-only data; see
benchmarks/scenarios.py for each scenario's shape).

Uses `pydart.Table.to_parquet`/`pydart.Table.from_parquet` (D-19: instance/classmethod shape,
matching `from_pandas`/`to_pandas`) via the pytest `tmp_path` fixture, mirroring
`tests/python/test_parquet_roundtrip.py`'s import and file-IO pattern. Two benchmark cases per
implementation: write (`to_parquet`) and read (`from_parquet`), each timed separately (matching
the from_pandas/to_pandas split, per BENCH-01's {from_pandas, to_pandas, read_parquet,
write_parquet} matrix axis).

D-40/T-03-09: the categorical_ordered/categorical_unordered scenarios feeding these Parquet-IO
cases carry a known fidelity gap -- see benchmarks/scenarios.py's inline comments and
BENCHMARKS.md's Known Limitations section. `.cat.categories` order and unused categories do NOT
survive a Parquet round-trip (values and `dict_is_ordered` DO); this benchmark measures speed
only, not round-trip fidelity (fidelity is already covered by
tests/python/test_parquet_fidelity.py).
"""

import pyarrow as pa
import pyarrow.parquet as pq
import pytest
from scenarios import SCENARIOS

import pydart


@pytest.mark.parametrize("scenario", SCENARIOS)
def test_write_parquet_pydart(benchmark, scenario, make_df, tmp_path):
    df = make_df(scenario)
    table = pydart.Table.from_pandas(df)
    path = tmp_path / f"{scenario}_pydart_write.parquet"
    benchmark(table.to_parquet, str(path))


@pytest.mark.parametrize("scenario", SCENARIOS)
def test_write_parquet_pyarrow(benchmark, scenario, make_df, tmp_path):
    df = make_df(scenario)
    pa_table = pa.Table.from_pandas(df)
    path = tmp_path / f"{scenario}_pyarrow_write.parquet"
    benchmark(pq.write_table, pa_table, str(path))


@pytest.mark.parametrize("scenario", SCENARIOS)
def test_read_parquet_pydart(benchmark, scenario, make_df, tmp_path):
    df = make_df(scenario)
    path = tmp_path / f"{scenario}_pydart_read.parquet"
    pydart.Table.from_pandas(df).to_parquet(str(path))
    benchmark(pydart.Table.from_parquet, str(path))


@pytest.mark.parametrize("scenario", SCENARIOS)
def test_read_parquet_pyarrow(benchmark, scenario, make_df, tmp_path):
    df = make_df(scenario)
    path = tmp_path / f"{scenario}_pyarrow_read.parquet"
    pq.write_table(pa.Table.from_pandas(df), str(path))
    benchmark(pq.read_table, str(path))
