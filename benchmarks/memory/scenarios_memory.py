"""BENCH-02 (D-39: synthetic-only): standalone subprocess entry point for peak-RSS measurement.

Re-imports `make_df`/`SCENARIOS` from `benchmarks/scenarios.py` (no duplicated generator -- single
source of scenario definitions, per this plan's key_links). Invoked as
`python scenarios_memory.py <scenario_name>` by `benchmarks/memory/measure_rss.py`'s
subprocess-isolated psutil harness -- this process is the one whose peak RSS gets measured, so it
must actually build the frame and run the real pydart conversion (not just print), so the
Rust-owned Arrow buffers being measured actually get allocated.
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


def run_scenario(scenario_name: str, n_rows: int = DEFAULT_N_ROWS) -> None:
    import pydart

    if scenario_name not in SCENARIOS:
        raise SystemExit(f"Unknown scenario: {scenario_name!r} (known: {SCENARIOS})")

    df = make_df(scenario_name, n_rows)
    table = pydart.Table.from_pandas(df)
    # Keep a live reference and briefly hold the process open so the parent's psutil poll loop
    # (benchmarks/memory/measure_rss.py) has a real chance to observe this process's peak RSS
    # before it exits -- a scenario that allocates and immediately exits could otherwise race
    # past the parent's first poll.
    _ = table.to_pandas()
    time.sleep(0.05)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit("usage: scenarios_memory.py <scenario_name>")
    run_scenario(sys.argv[1])
