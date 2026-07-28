"""Shared pytest fixtures for the benchmark suite (BENCH-01/BENCH-02, D-39: synthetic-only data).

Exposes `make_df` (see `benchmarks/scenarios.py`) as a fixture so benchmark test modules can
request it directly, mirroring the existing `tests/python/` fixture-building conventions.
"""

import pytest
from scenarios import make_df as _make_df


@pytest.fixture
def make_df():
    return _make_df
