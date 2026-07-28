"""BENCH-02 (D-39: synthetic-only): standalone subprocess entry point for peak-RSS measurement.

Re-imports `make_df`/`SCENARIOS` from `benchmarks/scenarios.py` (no duplicated generator -- single
source of scenario definitions, per this plan's key_links). Invoked as
`python scenarios_memory.py <scenario_name> [impl]` by `benchmarks/memory/measure_rss.py`'s
subprocess-isolated psutil harness -- this process is the one whose peak RSS gets measured, so it
must actually build the frame and run the real conversion (not just print), so the Rust-owned
Arrow buffers (`impl=pydart`) or pyarrow's own C++-allocated buffers (`impl=pyarrow`) being
measured actually get allocated.

`impl` (default `pydart`) selects which library's round trip is exercised -- BENCHMARKS.md's pass
bar compares pydart peak RSS against pyarrow peak RSS per scenario (`pydart peak RSS <= pyarrow
peak RSS` for true-zero-copy scenarios), which requires measuring both, not just pydart.
"""

import sys
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


def run_scenario(scenario_name: str, impl: str = "pydart", n_rows: int = DEFAULT_N_ROWS) -> None:
    if scenario_name not in SCENARIOS:
        raise SystemExit(f"Unknown scenario: {scenario_name!r} (known: {SCENARIOS})")
    if impl not in IMPLEMENTATIONS:
        raise SystemExit(f"Unknown impl: {impl!r} (known: {IMPLEMENTATIONS})")

    df = make_df(scenario_name, n_rows)

    if impl == "pydart":
        import pydart

        table = pydart.Table.from_pandas(df)
        _ = table.to_pandas()
    else:
        import pyarrow as pa

        pa_table = pa.Table.from_pandas(df)
        _ = pa_table.to_pandas()

    # Keep a live reference and briefly hold the process open so the parent's psutil poll loop
    # (benchmarks/memory/measure_rss.py) has a real chance to observe this process's peak RSS
    # before it exits -- a scenario that allocates and immediately exits could otherwise race
    # past the parent's first poll.
    time.sleep(0.05)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit("usage: scenarios_memory.py <scenario_name> [impl]")
    scenario_arg = sys.argv[1]
    impl_arg = sys.argv[2] if len(sys.argv) > 2 else "pydart"
    run_scenario(scenario_arg, impl_arg)
