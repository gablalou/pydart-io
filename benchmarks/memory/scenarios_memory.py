"""BENCH-02 (D-39: synthetic-only): standalone subprocess entry point for peak-RSS measurement.

Re-imports `make_df`/`SCENARIOS` from `benchmarks/scenarios.py` (no duplicated generator -- single
source of scenario definitions, per this plan's key_links). Invoked as
`python scenarios_memory.py <scenario_name> [impl] [axis]` by
`benchmarks/memory/measure_rss.py`'s subprocess-isolated psutil harness -- this process is the one
whose peak RSS gets measured, so it must actually build the frame and run the real conversion (not
just print), so the Rust-owned Arrow buffers (`impl=pydart`) or pyarrow's own C++-allocated
buffers (`impl=pyarrow`) being measured actually get allocated.

`impl` (default `pydart`) selects which library's operation is exercised -- BENCHMARKS.md's pass
bar compares pydart peak RSS against pyarrow peak RSS per scenario (`pydart peak RSS <= pyarrow
peak RSS` for true-zero-copy scenarios), which requires measuring both, not just pydart.

`axis` (default `round_trip`, preserving Plan 01's original behavior) selects which matrix axis to
measure -- `from_pandas`, `to_pandas`, `write_parquet`, `read_parquet`, or `round_trip`
(from_pandas + to_pandas combined, the Plan 01 default). BENCH-02's must_haves require every
matrix cell (not just the pandas<->Arrow round trip) to report peak RSS, so Parquet read/write
get their own axis values here, mirroring benchmarks/test_bench_parquet_io.py's write-then-read
split.
"""

import sys
import tempfile
import time
from pathlib import Path

# benchmarks/ (this file's parent's parent) holds scenarios.py. Add it to sys.path so this
# standalone script -- run directly via `python scenarios_memory.py`, not imported as part of a
# package -- can import it by bare module name. Mirrors pytest's own prepend-mode import of
# benchmarks/conftest.py (no `__init__.py` above it either).
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from scenarios import SCENARIOS, make_df  # noqa: E402

DEFAULT_N_ROWS = 1_000_000
IMPLEMENTATIONS = ["pydart", "pyarrow"]
AXES = ["round_trip", "from_pandas", "to_pandas", "write_parquet", "read_parquet"]


def run_scenario(
    scenario_name: str,
    impl: str = "pydart",
    axis: str = "round_trip",
    n_rows: int = DEFAULT_N_ROWS,
) -> None:
    if scenario_name not in SCENARIOS:
        raise SystemExit(f"Unknown scenario: {scenario_name!r} (known: {SCENARIOS})")
    if impl not in IMPLEMENTATIONS:
        raise SystemExit(f"Unknown impl: {impl!r} (known: {IMPLEMENTATIONS})")
    if axis not in AXES:
        raise SystemExit(f"Unknown axis: {axis!r} (known: {AXES})")

    df = make_df(scenario_name, n_rows)

    with tempfile.TemporaryDirectory() as tmp_dir:
        parquet_path = str(Path(tmp_dir) / f"{scenario_name}_{impl}.parquet")

        if impl == "pydart":
            import pydart

            if axis == "from_pandas":
                _ = pydart.Table.from_pandas(df)
            elif axis == "to_pandas":
                table = pydart.Table.from_pandas(df)
                _ = table.to_pandas()
            elif axis == "write_parquet":
                table = pydart.Table.from_pandas(df)
                table.to_parquet(parquet_path)
            elif axis == "read_parquet":
                pydart.Table.from_pandas(df).to_parquet(parquet_path)
                _ = pydart.Table.from_parquet(parquet_path)
            else:  # round_trip (Plan 01 default: from_pandas + to_pandas combined)
                table = pydart.Table.from_pandas(df)
                _ = table.to_pandas()
        else:
            import pyarrow as pa
            import pyarrow.parquet as pq

            if axis == "from_pandas":
                _ = pa.Table.from_pandas(df)
            elif axis == "to_pandas":
                pa_table = pa.Table.from_pandas(df)
                _ = pa_table.to_pandas()
            elif axis == "write_parquet":
                pa_table = pa.Table.from_pandas(df)
                pq.write_table(pa_table, parquet_path)
            elif axis == "read_parquet":
                pq.write_table(pa.Table.from_pandas(df), parquet_path)
                _ = pq.read_table(parquet_path)
            else:  # round_trip
                pa_table = pa.Table.from_pandas(df)
                _ = pa_table.to_pandas()

        # Keep a live reference and briefly hold the process open so the parent's psutil poll
        # loop (benchmarks/memory/measure_rss.py) has a real chance to observe this process's
        # peak RSS before it exits -- a scenario that allocates and immediately exits could
        # otherwise race past the parent's first poll.
        time.sleep(0.05)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit("usage: scenarios_memory.py <scenario_name> [impl] [axis]")
    scenario_arg = sys.argv[1]
    impl_arg = sys.argv[2] if len(sys.argv) > 2 else "pydart"
    axis_arg = sys.argv[3] if len(sys.argv) > 3 else "round_trip"
    run_scenario(scenario_arg, impl_arg, axis_arg)
