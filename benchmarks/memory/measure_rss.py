"""BENCH-02: peak-RSS memory measurement, subprocess-isolated per scenario.

Deliberately does NOT use any in-process Python-heap allocation profiler here -- pydart's whole
value proposition is that data lives in Rust-owned Arrow buffers outside the Python heap, so such
a profiler would report near-zero for exactly the memory this harness needs to measure (see
RESEARCH.md Pitfall 2 for the specific tool this rules out). `psutil`, measured in a fresh
subprocess per scenario, is the correct cross-platform (Linux/macOS/Windows) mechanism: a fresh
subprocess avoids a prior scenario's retained allocator arena contaminating the next scenario's
peak reading.
"""

import subprocess
import sys
from pathlib import Path

import psutil


def measure_peak_rss(
    scenario_script: str, scenario_name: str, impl: str = "pydart", axis: str = "round_trip"
) -> int:
    """Run `scenario_script` in a fresh subprocess; poll psutil for peak RSS.

    `impl` selects which library's operation `scenario_script` exercises (`pydart` or
    `pyarrow`); `axis` selects which matrix axis (`from_pandas`, `to_pandas`, `write_parquet`,
    `read_parquet`, or the Plan 01 default `round_trip`) -- BENCHMARKS.md's pass bar compares
    pydart vs pyarrow peak RSS per scenario/axis cell.

    Returns peak RSS in bytes.
    """
    proc = subprocess.Popen([sys.executable, scenario_script, scenario_name, impl, axis])
    p = psutil.Process(proc.pid)
    peak = 0
    while proc.poll() is None:
        try:
            peak = max(peak, p.memory_info().rss)
        except psutil.NoSuchProcess:
            break
    return peak


if __name__ == "__main__":
    scenario_arg = sys.argv[1] if len(sys.argv) > 1 else "numeric_dense"
    impl_arg = sys.argv[2] if len(sys.argv) > 2 else "pydart"
    axis_arg = sys.argv[3] if len(sys.argv) > 3 else "round_trip"
    scenario_script_path = str(Path(__file__).resolve().parent / "scenarios_memory.py")
    peak_bytes = measure_peak_rss(scenario_script_path, scenario_arg, impl_arg, axis_arg)
    print(f"{scenario_arg} ({impl_arg}, {axis_arg}): peak RSS = {peak_bytes} bytes")
