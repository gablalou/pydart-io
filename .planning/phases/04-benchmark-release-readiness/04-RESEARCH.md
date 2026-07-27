# Phase 4: Benchmark & Release Readiness - Research

**Researched:** 2026-07-27
**Domain:** Python packaging/release engineering (maturin, manylinux, PyPI trusted publishing, uv) + benchmark methodology (throughput and peak-memory comparison vs pyarrow)
**Confidence:** MEDIUM overall — packaging mechanics are HIGH (official docs, registry-verified versions); benchmark methodology specifics remain MEDIUM per the phase's own carried-forward flag in STATE.md. One finding below is a **blocking, HIGH-confidence correctness issue that contradicts a locked decision** — see Summary.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**PyPI Publishing & Package Identity**
- **D-32:** Phase 4 actually publishes the package to real PyPI — not build-and-verify-only, not TestPyPI-only. `pip install pydart` must work for real users by the end of this phase.
- **D-33:** The project is renamed from "flint" (placeholder) to **pydart**, finalized across the PyPI listing, Python import path (`import pydart`), Rust crates (`pydart-core`, `pydart-python`), the compiled extension (`pydart._pydart`), the Python-visible exception (`pydart.PydartError`), all tests, and docs. This was executed as a **prerequisite quick task** rather than as Phase 4's first plan task. All downstream agents MUST use "pydart" as the project/package name — "flint" no longer exists in the codebase except in historical `.planning/` narrative.

**Platform, Architecture & Version Floor**
- **D-34:** Wheels target **x86_64 AND aarch64/arm64** for Linux (manylinux) and macOS, plus Windows x86_64 — not x86_64-only.
- **D-35:** `requires-python` in `pyproject.toml` is corrected from the stale `>=3.12` to **`>=3.11`**, matching the actual compiled `abi3-py311` wheel floor. Dev-dependency pins (numpy, pandas, etc.) must be re-resolved to actually work under Python 3.11, not just documented as compatible — this is a real task, not a one-line edit.
- **D-36:** The pandas floor is pinned to **`>=3.0`** (not `>=2.2`) specifically to close the WR-02 blocker recorded in STATE.md: pandas' Copy-on-Write (CoW) is unconditional only from pandas 3.0 onward, and the zero-copy numpy buffer borrow's post-borrow-mutation safety relies entirely on CoW being active. `.claude/CLAUDE.md`'s "pandas >= 2.2" support claim and `pyproject.toml`'s dependency floor both need updating.
- **D-37:** The CI version matrix (PKG-02) spans oldest-to-newest supported numpy/pandas versions within the corrected floors (Python >=3.11, pandas >=3.0). Exact version list is Claude's/planner's discretion.

**Benchmark Claim Presentation & Data Realism**
- **D-38:** Benchmark results are published as a **committed `BENCHMARKS.md` in the repo** — methodology plus raw numbers, regenerated per release — not an external CI-service dashboard (e.g. CodSpeed).
- **D-39:** Benchmark data is **synthetic-only** for v1 — no public/real-world dataset download. All scenarios generated programmatically with controlled, documented shapes/row counts.
- **D-40:** Categorical/dictionary columns **ARE included** in the benchmark matrix, including Parquet-IO scenarios, **with an explicit caveat** in `BENCHMARKS.md`/release docs calling out T-03-09: categorical `.cat.categories` order and unused-category retention do NOT survive a Parquet round-trip (values and `dict_is_ordered` DO survive correctly).

### Claude's Discretion
- Exact benchmark tooling: pytest-benchmark vs CodSpeed's local/self-hosted mode vs a custom timing harness for Python-level benchmarks; `criterion` for Rust-side kernels.
- Memory-measurement method (`tracemalloc`, `resource.getrusage`, `psutil`, or a Rust-side allocation counter) and handling cross-platform RSS-measurement quirks (Windows differs from Linux/macOS).
- manylinux glibc tag choice (`manylinux2014` vs `manylinux_2_28`).
- CI provider setup mechanics — GitHub Actions assumed (repo hosted on GitHub; `maturin-action` is GitHub-specific). No `.github/workflows/` exists yet — greenfield.
- Exact numpy/pandas version list for the CI compatibility matrix (D-37).
- PyPI trusted-publisher (OIDC) vs classic API-token authentication, and release-tagging mechanism (git tag trigger vs manual workflow dispatch).

### Deferred Ideas (OUT OF SCOPE)
- **External CI-service benchmark dashboard (e.g. CodSpeed)** — considered but not chosen for v1; `BENCHMARKS.md` in-repo is the canonical claim (D-38). Revisit if ongoing PR-level regression tracking becomes valuable post-release.
- **Semi-realistic public dataset (e.g. NYC taxi) for benchmarks** — deferred; synthetic-only for v1 (D-39).
- PyPI trusted-publisher mechanics and release-tagging strategy are **not** deferred — they're in scope, just left to Claude's/planner's discretion (see above).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| BENCH-01 | Benchmark suite compares this library vs pyarrow across a realistic matrix (numeric, mixed, nullable, chunked, object-string scenarios) | See Standard Stack (pytest-benchmark + criterion split), Architecture Patterns (Pattern 1: scenario matrix), Pitfall 1 (blended-claim trap), Code Examples |
| BENCH-02 | Benchmark suite reports both throughput and peak memory (RSS), not just speed | See Pitfall 2 (tracemalloc is the wrong tool), Code Examples (psutil subprocess-isolated RSS measurement) |
| PKG-01 | Project builds installable wheels for manylinux, macOS, and Windows via maturin | See Standard Stack (maturin-action), Architecture Patterns (Pattern 2: CI build matrix), Pitfall 3 (ABI/manylinux tag pitfalls), Pitfall 6 (PyPI name collision — blocks D-32/D-33 as currently locked) |
| PKG-02 | CI tests against a version matrix of supported numpy/pandas versions (oldest to newest supported) | See Standard Stack (verified version floors), Pitfall 4 (pinned-version resolver conflict), Code Examples |
| PKG-03 | Package installs cleanly via `uv` (`uv add`/`uv pip install`), and the dev environment (lockfile, build/test commands) is uv-compatible | See Standard Stack (uv universal resolution), Pitfall 4, Code Examples |
</phase_requirements>

## Summary

This phase's job is narrow but release-gating: build a defensible benchmark suite and ship real, installable wheels — no new conversion/IO logic. Most of the mechanics (maturin, manylinux, PyO3 abi3, arrow-rs versions) were already locked in Phase 1-3 research; what's genuinely new here is (a) a benchmark harness that reports both time *and* RSS across a realistic dtype matrix, and (b) a CI/release pipeline (wheel matrix + version matrix + PyPI publish) that doesn't exist yet in this repo (`.github/` is empty).

**Blocking finding, discovered during this research session:** `pydart` is **already registered on PyPI** — it belongs to an unrelated project ("Python Interface for DART Simulator" by Sehoon Ha, `github.com/sehoonha/pydart`) `[VERIFIED: PyPI registry, https://pypi.org/pypi/pydart/json, HTTP 200 returned 2026-07-27]`. D-32 requires `pip install pydart` to work for real users, and D-33 locks "pydart" as the finalized project/package name across crates, Python import path, and PyPI listing. **These two locked decisions cannot both be satisfied as written** — the name is not claimable on the real PyPI index. This must be surfaced to the user/planner as a blocking decision before Phase 4 plans a publish pipeline around an unavailable name (see Pitfall 6 and Open Questions).

Separately, `requires-python >=3.11` (D-35) does not resolve cleanly against the current `pyproject.toml`: the pinned dev dependency `numpy==2.5.1` requires Python **>=3.12** `[VERIFIED: PyPI registry api, pypi.org/pypi/numpy/2.5.1/json]` — it dropped 3.11 support. The concrete, registry-verified fix is a numpy dev-pin downgrade/loosen to a version that still supports 3.11 (see Pitfall 4).

For the benchmark suite: `tracemalloc` (Python-heap-only) is structurally the wrong tool for BENCH-02, because pydart's whole value proposition is that data lives in Rust-owned Arrow buffers outside the Python heap — it would report near-zero for exactly the memory this phase needs to measure. Peak RSS via `psutil`, measured in a subprocess per scenario, is the correct cross-platform mechanism (see Pitfall 2).

**Primary recommendation:** Treat this phase's plan as two independent tracks that can be built in parallel (mirrors the Phase 1-3 architecture split of Arrow-core vs pandas-interop vs Parquet-IO): (1) a benchmark harness (`criterion` for Rust kernels + `pytest-benchmark` for Python-facing throughput + `psutil`-subprocess for RSS, assembled into a committed `BENCHMARKS.md`), and (2) a packaging/release pipeline (`maturin-action` wheel matrix across manylinux2014 x86_64/aarch64 + macOS x86_64/arm64 + Windows x86_64, a numpy/pandas compatibility-matrix CI job, and PyPI OIDC trusted publishing) — but track (2) cannot complete under the current package-name lock and needs a decision from the user before planning proceeds on that half.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Rust-side micro-benchmarks (pure conversion/Parquet kernels, no GIL) | Rust core (`pydart-core`) | — | Isolates "is slowness in Rust or at the PyO3/GIL boundary" per CLAUDE.md's `criterion` rationale; belongs in the pyo3-free crate |
| Python-facing throughput benchmarks (`from_pandas`/`to_pandas`/`read_parquet` vs pyarrow equivalents) | Python test/bench harness (`benchmarks/` or `tests/benchmarks/`) | PyO3 binding layer (exercised, not modified) | Measures the full user-visible call path including FFI/GIL overhead — this is the number that goes in `BENCHMARKS.md` |
| Peak-memory (RSS) measurement | OS process tier (subprocess + `psutil`) | Python harness (orchestrates subprocess spawn/collect) | RSS is a whole-process OS metric; must be measured from *outside* the interpreter whose heap is being profiled, and in a fresh subprocess per scenario so allocator retention from a prior scenario doesn't contaminate the next measurement |
| Wheel build (compile + package) | CI / build tier (`maturin-action`) | Rust core + PyO3 binding (the thing being compiled) | Build-time concern, orthogonal to runtime architecture; owned entirely by CI config, not application code |
| Package publish (PyPI upload) | CI / release tier (`gh-action-pypi-publish` via OIDC) | — | Supply-chain/identity concern — should not touch application code at all |
| Version-compatibility testing (numpy/pandas matrix) | CI / test tier | Python-facing public API (what's actually being exercised per matrix cell) | Confirms the ABI/dtype-mapping assumptions in `pandas.rs` hold across the declared support range, not a new capability |
| `uv` dev/lockfile workflow | Dev-tooling tier (`pyproject.toml` + `uv.lock`) | — | Affects reproducibility of local dev and CI installs, not runtime behavior |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|---------------|
| `pytest-benchmark` | 5.2.3 `[VERIFIED: PyPI registry, pip index versions]` | Python-level throughput benchmarking (statistically defensible repeated-trial timing, not single-shot `time.time()`) | Already recommended in `.claude/CLAUDE.md`; matches D-38's "committed markdown, no external CI-service dependency" decision — it's a local pytest plugin, no dashboard upload required |
| `criterion` | 0.8.2 `[VERIFIED: crates.io registry api]` | Rust-side micro-benchmarks for pure `pydart-core` conversion/Parquet kernels, isolated from PyO3/GIL overhead | Standard Rust benchmarking crate; CLAUDE.md's explicit recommendation for separating "slow in Rust" from "slow at the FFI boundary" |
| `psutil` | 7.2.2 `[VERIFIED: PyPI registry, pip index versions]` | Cross-platform peak-RSS measurement for BENCH-02 (subprocess-isolated per scenario) | The only practical cross-platform (Linux/macOS/Windows) process-memory API in the Python ecosystem; `resource.getrusage` is Unix-only with inconsistent units (KB on Linux, bytes on BSD/macOS) and doesn't exist on Windows at all |
| `maturin` | already pinned 1.14.1 in `pyproject.toml`/`Cargo.toml` — no change needed | Build backend; also drives the manylinux/abi3 wheel-tagging CI needs | Already the project's build backend since Phase 1; this phase's new use is invoking it repeatedly across a CI target matrix, not changing it |
| `maturin-action` | v1.51.0 `[VERIFIED: GitHub tags API, github.com/PyO3/maturin-action]` | GitHub Action wrapping `maturin build`/`publish` with built-in cross-compilation and manylinux container handling | PyO3-org-maintained, purpose-built for exactly this project's build shape (single Rust extension, no other native deps) — see Alternatives Considered for why not `cibuildwheel` |
| `gh-action-pypi-publish` | v1.14.1 `[VERIFIED: GitHub tags API, github.com/pypa/gh-action-pypi-publish]` | The PyPA-maintained "blessed" action for uploading wheels/sdist to PyPI via OIDC trusted publishing (no long-lived API token in CI secrets) | Official PyPA action; OIDC trusted publishing is the current (2024+) best-practice replacement for API-token-in-secrets, eliminates a class of supply-chain risk (leaked/stale token) |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `pytest-codspeed` | 5.0.3 `[VERIFIED: PyPI registry, pip index versions]` | Alternative/companion benchmark runner with CodSpeed-compatible output | Only if the project later wants CodSpeed's local/self-hosted regression detection without the external dashboard upload — not required for D-38's committed-markdown decision; do not add unless the planner has a specific reason, since `pytest-benchmark` alone already satisfies BENCH-01 |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `maturin-action` | `cibuildwheel` | More general-purpose (works with any PEP 517 backend, more platforms), but for a pure-Rust-extension project `maturin-action` is purpose-built and faster (builds the compiled artifact once, cross-compiles); prefer `cibuildwheel` only if the project later grows a mixed Rust+other-native-dependency build |
| `psutil`-subprocess RSS measurement | `resource.getrusage(RUSAGE_SELF).ru_maxrss` in-process | Simpler, no extra dependency, but Unix-only (`resource` module doesn't exist on Windows) and unit-inconsistent (KB on Linux vs bytes on macOS/BSD) — `psutil` normalizes this; use `resource` only as a documented Linux/macOS-only fallback if `psutil` is somehow unavailable |
| manylinux2014 | manylinux_2_28 | `manylinux_2_28` gives a newer glibc baseline (smaller wheels, some newer syscalls) but drops compatibility with older enterprise Linux distros still in the wild; `manylinux2014` is Rust's own documented minimum (Rust >=1.64 requires glibc >=2.17) and matches what pyarrow/polars/duckdb still ship broadly — safer default unless a specific reason favors the newer tag |
| PyPI OIDC trusted publishing | Classic API token in a repo secret | Token-based auth requires manually rotating a long-lived credential and is a documented supply-chain risk if leaked; OIDC issues a short-lived (minutes), workflow-run-scoped credential with nothing to leak or rotate — current PyPI/GitHub-recommended default for new projects |

**Installation:**
```bash
uv add --dev pytest-benchmark psutil
# pytest-codspeed only if the planner decides to add it later — not required for D-38
```

Rust side (`crates/pydart-core/Cargo.toml`):
```toml
[dev-dependencies]
criterion = "0.8"

[[bench]]
name = "conversion_bench"
harness = false
```

**Version verification:** All versions above were checked directly against the PyPI JSON API (`pip index versions <pkg>` / `pypi.org/pypi/<pkg>/json`) and the crates.io/GitHub tags API on 2026-07-27, not recalled from training data. `maturin` itself is unchanged from the already-pinned 1.14.1 (Phase 1 research); no re-verification needed since this phase doesn't touch the build backend version, only its CI invocation surface.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|--------------|---------|-------------|
| `pytest-benchmark` | PyPI | published 2025-11-09 (recent release of a long-established package; project itself is 10+ years old) | unknown (seam tool reported `unknown-downloads` — no network access to PyPI stats API in this environment) | not resolved by seam (`repoUrl: null`) — actual repo is `github.com/ionelmc/pytest-benchmark`, well-established, widely used | SUS (`unknown-downloads`, `no-repository`) | Approved with caveat — see below |
| `psutil` | PyPI | published 2026-01-28 (recent release; project is 15+ years old) | unknown (`unknown-downloads`) | `github.com/giampaolo/psutil` — confirmed, well-known | SUS (`unknown-downloads`) | Approved with caveat — see below |
| `pytest-codspeed` | PyPI | published 2026-05-22 | unknown (`unknown-downloads`) | `github.com/CodSpeedHQ/pytest-codspeed` — confirmed | SUS (`unknown-downloads`) | Approved with caveat (only if planner chooses to add it) |

**Packages removed due to [SLOP] verdict:** none

**Packages flagged as suspicious [SUS]:** `pytest-benchmark`, `psutil`, `pytest-codspeed` — all three flagged solely because the legitimacy-check tool could not retrieve download-count telemetry in this sandboxed environment (`unknown-downloads`), not because of any adversarial signal. Cross-referenced manually: all three are long-established, widely-used packages with legitimate public GitHub source repos (`ionelmc/pytest-benchmark`, `giampaolo/psutil`, `CodSpeedHQ/pytest-codspeed`) and version histories going back years to a decade-plus `[CITED: PyPI version history via pip index versions]`. Per protocol, the planner must still insert a `checkpoint:human-verify` task before each install, even though this researcher's manual cross-check found no red flags — the SUS verdict is a data-availability artifact, not a substantive concern.

## Architecture Patterns

### System Architecture Diagram

```
                         ┌─────────────────────────────────────────┐
                         │        Developer / CI trigger             │
                         │  (git tag push OR workflow_dispatch)      │
                         └───────────────┬───────────────────────────┘
                                          │
                 ┌────────────────────────┼────────────────────────┐
                 ▼                        ▼                        ▼
       ┌──────────────────┐   ┌────────────────────────┐  ┌──────────────────┐
       │  Benchmark job     │   │  Wheel build matrix     │  │ Compat-matrix job │
       │  (pytest run,      │   │  (maturin-action x      │  │ (numpy/pandas     │
       │  criterion run)    │   │   {linux x86_64/arm64,  │  │  oldest..newest,  │
       │                    │   │    macOS x86_64/arm64,  │  │  py 3.11/3.12+)   │
       │                    │   │    windows x86_64})     │  │                   │
       └─────────┬──────────┘   └───────────┬─────────────┘  └─────────┬─────────┘
                 │ scenario results          │ built wheels             │ pass/fail
                 ▼                           ▼                          ▼
       ┌──────────────────┐        ┌──────────────────┐       ┌──────────────────┐
       │ BENCHMARKS.md      │        │ Wheel artifacts    │       │ CI gate (blocks   │
       │ (committed, regen  │        │ (attached to       │       │ publish job on    │
       │  per release)      │        │  release)          │       │  any matrix fail) │
       └──────────────────┘        └─────────┬─────────┘       └─────────┬─────────┘
                                              │                            │
                                              └──────────────┬─────────────┘
                                                               ▼
                                                    ┌────────────────────────┐
                                                    │  Publish job            │
                                                    │  (gh-action-pypi-publish│
                                                    │   via OIDC, id-token:   │
                                                    │   write)                │
                                                    └───────────┬─────────────┘
                                                                ▼
                                                    ┌────────────────────────┐
                                                    │  Real PyPI               │
                                                    │  `pip install pydart`   │  <-- BLOCKED: name taken
                                                    └────────────────────────┘
```

### Recommended Project Structure
```
benchmarks/                       # NEW — Python-level benchmark harness
├── conftest.py                   # shared synthetic-data fixtures (D-39: generated, not downloaded)
├── scenarios.py                  # numeric / mixed / nullable / chunked / object-string / categorical shape generators
├── test_bench_from_pandas.py     # pytest-benchmark throughput cases: pydart vs pyarrow
├── test_bench_to_pandas.py
├── test_bench_parquet_io.py
└── memory/
    ├── measure_rss.py            # subprocess-isolated psutil peak-RSS harness (see Code Examples)
    └── scenarios_memory.py       # same scenario generators, reused for memory runs

crates/pydart-core/benches/       # NEW — Rust-side criterion micro-benchmarks
└── conversion_bench.rs

.github/workflows/                # NEW — greenfield, no prior patterns to follow/conflict with
├── ci.yml                        # cargo test + maturin develop + pytest (existing test_command)
├── compat-matrix.yml             # PKG-02: numpy/pandas version matrix
├── wheels.yml                    # PKG-01: maturin-action build matrix, artifact upload
└── release.yml                   # PKG-03/D-32: tag-triggered wheel build + OIDC publish

BENCHMARKS.md                     # NEW — D-38: committed methodology + raw numbers, regenerated per release
```

### Pattern 1: Scenario-matrix benchmark harness, not a single headline number

**What:** Structure the benchmark suite as a matrix — {numeric, mixed, nullable, chunked, object-string, categorical} x {from_pandas, to_pandas, read_parquet, write_parquet} — with every cell reporting both throughput and peak RSS, rather than one aggregate "N times faster than pyarrow" number.
**When to use:** Always for BENCH-01/BENCH-02 — this is the direct fix for Pitfall 6 from the project's own init research ("Benchmark claims that don't survive scrutiny").
**Example:**
```python
# Source: pattern derived from pytest-benchmark docs + project's own STACK.md Critical Caveat
import pytest

SCENARIOS = ["numeric_dense", "numeric_nullable", "mixed_object_string",
             "chunked_multi_batch", "categorical_ordered", "categorical_unordered"]

@pytest.mark.parametrize("scenario", SCENARIOS)
def test_from_pandas_pydart(benchmark, scenario, make_df):
    df = make_df(scenario, n_rows=1_000_000)
    benchmark(pydart.from_pandas, df)

@pytest.mark.parametrize("scenario", SCENARIOS)
def test_from_pandas_pyarrow(benchmark, scenario, make_df):
    df = make_df(scenario, n_rows=1_000_000)
    benchmark(pyarrow.Table.from_pandas, df)
```

### Pattern 2: CI build matrix mirrors the locked platform/arch decision (D-34) exactly

**What:** One `maturin-action` job per (OS, arch) cell: `ubuntu-latest` (manylinux x86_64), `ubuntu-24.04-arm` (manylinux aarch64, native runner — see Pitfall 5), `macos-13` (x86_64) + `macos-14` (arm64), `windows-latest` (x86_64).
**When to use:** PKG-01. Do not attempt a single "universal" job — GitHub Actions runners are architecture-specific, and `macos-14`/`macos-13` are the standard way to get both macOS arches without cross-compilation `[CITED: general web search cross-corroborated across maturin/cibuildwheel community sources, 2026-07-27]`.
**Example:**
```yaml
# Source: pattern derived from PyO3/maturin-action README + GitHub Actions arm64-runner changelog
strategy:
  matrix:
    include:
      - runner: ubuntu-latest
        target: x86_64-unknown-linux-gnu
        manylinux: "2014"
      - runner: ubuntu-24.04-arm
        target: aarch64-unknown-linux-gnu
        manylinux: "2014"
      - runner: macos-13
        target: x86_64-apple-darwin
      - runner: macos-14
        target: aarch64-apple-darwin
      - runner: windows-latest
        target: x86_64-pc-windows-msvc
runs-on: ${{ matrix.runner }}
steps:
  - uses: PyO3/maturin-action@v1
    with:
      target: ${{ matrix.target }}
      manylinux: ${{ matrix.manylinux || 'auto' }}
      args: --release --out dist
```

### Pattern 3: PyPI OIDC trusted publishing (no stored token)

**What:** A dedicated `publish` job with `permissions: id-token: write`, gated behind the full build+test matrix passing, using `pypa/gh-action-pypi-publish` — PyPI is configured (via its web UI, Settings -> Publishing) to trust this exact repo+workflow+environment combination, so no `PYPI_API_TOKEN` secret exists anywhere in the repo.
**When to use:** D-32's actual-publish requirement; this is Claude's-discretion territory per CONTEXT.md but OIDC is the strictly-safer default with no real downside for a from-scratch release pipeline.
**Example:**
```yaml
# Source: docs.pypi.org/trusted-publishers/using-a-publisher/ ; github.com/pypa/gh-action-pypi-publish
publish:
  needs: [wheels, compat-matrix]
  runs-on: ubuntu-latest
  environment: pypi
  permissions:
    id-token: write
  steps:
    - uses: actions/download-artifact@v4
      with: { pattern: wheels-*, path: dist, merge-multiple: true }
    - uses: pypa/gh-action-pypi-publish@release/v1
```

### Anti-Patterns to Avoid
- **Single blended "N% faster" headline number:** Masks that pydart is only truly zero-copy for numeric/non-null/Arrow-backed columns (per STACK.md's Critical Caveat) — always report per-scenario, per-dtype-family results.
- **Measuring RSS with `tracemalloc` or in-process:** Reports near-zero for the Rust-owned Arrow buffers that are the whole point of measuring — see Pitfall 2.
- **CI matrix testing only "latest" numpy/pandas:** Directly the Pitfall 7 failure mode called out in this project's own init PITFALLS.md — field-only ABI breaks that never show up on the maintainer's machine.
- **Building against the newest numpy for the compiled wheel:** NumPy's ABI is forward-compatible, not backward-compatible — build against the oldest numpy in the supported range, not whatever CI happens to have installed by default.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|--------------|-----|
| Statistically defensible timing (warmup, repeated trials, outlier handling) | A custom `time.perf_counter()` loop | `pytest-benchmark` | Single-shot timing is exactly the "benchmark claims that don't survive scrutiny" failure mode this phase's success criteria exist to prevent |
| Cross-platform peak-memory measurement | Hand-rolled `/proc/self/status` parsing or platform-specific branches | `psutil.Process().memory_info().rss`, sampled in a loop or measured via a fresh subprocess's peak | `psutil` already normalizes Linux/macOS/Windows differences (units, API availability) that a hand-rolled version would get wrong on at least one platform |
| Wheel manylinux/auditwheel compliance | Manual `patchelf`/`auditwheel repair` scripting | `maturin`'s built-in auditwheel-equivalent (already used since Phase 1) + `maturin-action`'s manylinux container handling | `maturin` already owns this; this phase just needs to invoke it across a CI matrix, not reimplement compliance checking |
| PyPI upload authentication | A stored `PYPI_API_TOKEN` repo secret + manual `twine upload` | OIDC trusted publishing via `gh-action-pypi-publish` | Long-lived tokens are a documented supply-chain risk (leak, staleness); OIDC's short-lived per-run credential structurally eliminates that risk class |

**Key insight:** Every "don't hand-roll" item above already has a project-recommended or PyPA/GitHub-blessed tool; this phase's actual engineering effort should go into the benchmark *scenario design* (the dtype matrix) and the CI *matrix wiring* (which cells to cover), not into re-implementing timing, memory-measurement, or packaging mechanics that are already solved problems.

## Common Pitfalls

### Pitfall 1: Blended benchmark claim hides the honest zero-copy-vs-copy split
**What goes wrong:** Reporting a single "pydart is Nx faster than pyarrow" number averages together the cases where pydart is genuinely zero-copy (huge, honest win) with the cases where it must copy just like pyarrow does (small or no win, sometimes a loss if pydart's copy path is less mature than pyarrow's years-optimized C++ code).
**Why it happens:** A single headline number is more marketable, and it's tempting to report the best-case scenario as "the" number.
**How to avoid:** Report every scenario cell separately in `BENCHMARKS.md`; explicitly label which scenarios are "true zero-copy" vs "minimal-copy fallback" per STACK.md's own Critical Caveat.
**Warning signs:** A benchmark report with one number and no per-dtype breakdown table.

### Pitfall 2: `tracemalloc` (or any in-process Python-heap profiler) cannot see pydart's actual memory cost
**What goes wrong:** `tracemalloc` instruments Python's own allocator; the Arrow buffers pydart allocates live in Rust-owned, non-Python-heap memory (that's the entire point of zero-copy). Using `tracemalloc` for BENCH-02 would report near-zero bytes for exactly the memory the phase needs to measure and compare against pyarrow's (also mostly non-Python-heap, C++-allocated) memory.
**Why it happens:** `tracemalloc` is the first tool that comes to mind for "measure Python memory usage," and it does work correctly for pure-Python code — the mismatch only appears because pydart's core value is data living outside the Python heap.
**How to avoid:** Measure whole-process RSS via `psutil`, and measure it in a **fresh subprocess per scenario** (not a shared long-running process) so a prior scenario's retained allocator arena doesn't inflate/deflate the next scenario's peak reading. `resource.getrusage(RUSAGE_SELF).ru_maxrss` is a viable Linux/macOS-only fallback (note: KB on Linux, bytes on macOS/BSD — must normalize) but has no Windows equivalent at all `[CITED: general web search, psutil issue tracker giampaolo/psutil#1096]`.
**Warning signs:** A memory benchmark reporting near-identical, near-zero numbers for every scenario regardless of DataFrame size — a sign the wrong memory is being measured.

### Pitfall 3: Building wheels against the newest numpy breaks users on older numpy (ABI is forward-, not backward-, compatible)
**What goes wrong:** A wheel compiled with the CI runner's default/latest numpy (e.g. via `rust-numpy`) can segfault or hit an `undefined symbol`/`NULL` C-API slot for users who have an older numpy installed, because numpy's C API is forward-compatible (old code works with new numpy) but not backward-compatible (new-built code can break on old numpy).
**Why it happens:** CI naturally installs "latest" of everything by default unless explicitly pinned to the floor version during the *build* step (distinct from the test matrix, which should span old-to-new).
**How to avoid:** Ensure the actual wheel-build job (not just the test-matrix job) builds against the project's declared numpy floor, not whatever's newest at build time. This is inherited directly from the project's own init research (`.planning/research/PITFALLS.md` Pitfall 7).
**Warning signs:** Field bug reports of segfaults/import errors that only reproduce on users' machines, never the maintainer's/CI's.

### Pitfall 4: The current `pyproject.toml` dev-dependency pin structurally conflicts with the locked D-35 Python floor
**What goes wrong:** `pyproject.toml` currently pins `numpy==2.5.1` as an exact dev dependency, with an explicit comment noting this requires Python >=3.12. `[VERIFIED: PyPI registry api, https://pypi.org/pypi/numpy/2.5.1/json — requires_python: ">=3.12"]`. D-35 locks `requires-python = ">=3.11"`. Under `uv`'s resolver, an exact pin (`==2.5.1`) that itself requires `>=3.12` will make the *whole* dependency set fail to resolve for a 3.11 interpreter — this is not a hypothetical, it's the exact same failure mode that produced the original `>=3.12` floor per STATE.md's own decision log ("Set pyproject.toml requires-python to >=3.12 to satisfy the RESEARCH.md-pinned numpy==2.5.1 dev dependency under uv's resolver").
**Why it happens:** Exact version pins (`==`) are simple to reason about for a single-Python-version dev environment, but they don't degrade gracefully when the floor changes underneath them.
**How to avoid:** Registry-verified fix: numpy versions **2.3.0 through 2.4.1** declare `requires_python: ">=3.11"` `[VERIFIED: PyPI registry api]` — numpy **2.5.0/2.5.1 are the first releases to drop 3.11 support** and require `>=3.12`. Loosen the dev-dependency pin to a range (e.g. `numpy>=2.3,<2.6`) rather than an exact version, so `uv`'s **universal resolver** (which is documented to select different concrete versions per Python-version marker within a single lockfile `[CITED: general web search, uv resolution docs synthesis]`) can pick numpy 2.4.x for a 3.11 interpreter and 2.5.x for 3.12+, all within one `uv.lock`. Apply the same treatment to any other exact-pinned dev dependency that might have a similar floor mismatch (spot-checked: `pandas==3.0.3` requires `>=3.11` `[VERIFIED: PyPI registry api, pandas 3.0.0 requires_python ">=3.11"]` — no conflict there).
**Warning signs:** `uv sync`/`uv lock` failing with a resolution error mentioning `requires-python` incompatibility as soon as the floor is edited.

### Pitfall 5: aarch64 Linux wheels built under QEMU emulation are dramatically slower to build and often skipped in tests
**What goes wrong:** `maturin-action` can cross-compile aarch64 wheels on an x86_64 runner via QEMU emulation, but emulated builds/tests can take an order of magnitude longer than native, and it's common practice to build-but-not-test under QEMU (since the compiled binary can't be *run* on the emulated architecture practically for a full test suite).
**Why it happens:** Historically, GitHub Actions had no native ARM64 hosted runner, so QEMU cross-compilation was the only option.
**How to avoid:** GitHub now provides **native arm64 Linux hosted runners for free on public repositories** (`ubuntu-24.04-arm`, `ubuntu-22.04-arm`) as of the 2025 GA rollout `[CITED: github.blog/changelog/2025-08-07-arm64-hosted-runners-for-public-repositories-are-now-generally-available/]`. Use `ubuntu-24.04-arm` directly as the aarch64 job's `runs-on`, which builds *and can run/test* the wheel natively — no QEMU, no skip-tests compromise. Confirm this repo is public (required for the free native-arm tier); if private, private-repo arm64 runners became available January 2026 but likely carry different billing.
**Warning signs:** A CI config using `docker/setup-qemu-action` for the aarch64 build when a native runner label would work directly — unnecessary complexity/slowness for this project's actual CI needs.

### Pitfall 6 (BLOCKING — surfaced to user, not silently worked around): `pydart` is not available on the real PyPI index
**What goes wrong:** D-32 requires `pip install pydart` to work for real users; D-33 finalizes "pydart" as the PyPI listing name. The name `pydart` is **already registered** on PyPI by an unrelated, pre-existing project (`Python Interface for DART Simulator`, author Sehoon Ha, `github.com/sehoonha/pydart`) `[VERIFIED: PyPI registry, curl https://pypi.org/pypi/pydart/json returned HTTP 200 with that project's metadata, checked 2026-07-27]`. PyPI does not allow two different projects to claim the same name — this is not a resolvable technical problem, it's a naming collision that must be resolved by the user before the publish step (PKG-01/D-32) can execute as currently scoped.
**Why it happens:** The rename discussion (D-33) evaluated "pydart" for its descriptive fit and its availability as a Python import path / Rust crate name (which have no global registry and were confirmed free), but did not check the actual PyPI package-name registry before locking the decision.
**How to avoid:** This is an **Open Question requiring a user decision before planning proceeds on the publish half of this phase** — options include: (a) publish under a different, available PyPI name while keeping `import pydart` as the Python import path (PyPI package name and the importable module name do not have to match — this is common practice, e.g. `pip install some-pkg` then `import somepkg`), (b) choose a differentiated name for both PyPI and the import path (re-open D-33), or (c) contact the existing `pydart` PyPI project's owner about name transfer (slow, uncertain, not a v1-timeline-compatible option). See Open Questions below — the planner must not silently pick one of these; it is a locked-decision conflict, not a routine implementation detail.
**Warning signs:** A publish workflow that assumes `pydart` will succeed on first `twine upload`/`gh-action-pypi-publish` run — it will fail with a 403/name-already-claimed error, which is avoidable by checking this before building the pipeline around it.

## Runtime State Inventory

None — this is not a rename/refactor/migration phase (the flint->pydart rename was already completed and verified as a prerequisite quick task per D-33). However, one related release-hygiene finding surfaced during this research: **stale pre-rename build artifacts still exist on disk** and are untracked by git — `python/flint/` (containing a stale `_flint.abi3.so`), `target/wheels/flint-0.1.0-cp311-abi3-linux_x86_64.whl`, `target/debug/lib_flint.so`/`libflint_core.rlib`, `target/release/lib_flint.so`, and `target/doc/flint_core`/`target/doc/_flint`. None of these are tracked in git (confirmed via `git status`) and `pyproject.toml`'s `python-source = "python"` + `module-name = "pydart._pydart"` config means the current build only packages `python/pydart/`, not `python/flint/` — so there is **no wheel-contamination risk** from these leftovers. Still, flag as a pre-flight cleanup step (`rm -rf python/flint target/wheels/flint-*.whl target/debug/*flint* target/release/*flint* target/doc/*flint*` or simply `cargo clean && rm -rf python/flint`) before Phase 4's wheel-build CI is stood up, so a stale artifact from a local dev machine's `target/` doesn't accidentally get included in a manually-triggered local build/publish test.

## Code Examples

### RSS-isolated memory measurement (BENCH-02)
```python
# Source: pattern synthesized from psutil docs (psutil.readthedocs.io) + general
# cross-platform-process-memory best practice (no single official "how to measure
# peak RSS of a Python subprocess" doc exists; this is MEDIUM confidence, standard practice)
import subprocess, sys, json

def measure_peak_rss(scenario_script: str, scenario_name: str) -> int:
    """Run scenario_script in a fresh subprocess; poll psutil for peak RSS.
    Returns peak RSS in bytes. Fresh subprocess per scenario avoids allocator
    retention from a prior run contaminating the next measurement."""
    import psutil
    proc = subprocess.Popen([sys.executable, scenario_script, scenario_name])
    p = psutil.Process(proc.pid)
    peak = 0
    while proc.poll() is None:
        try:
            peak = max(peak, p.memory_info().rss)
        except psutil.NoSuchProcess:
            break
    return peak
```

### Rust-side criterion micro-benchmark skeleton
```rust
// Source: criterion.rs docs pattern (bheisler/criterion.rs), applied to pydart-core
use criterion::{criterion_group, criterion_main, Criterion};
use pydart_core::conversion::from_numpy_buffer; // pyo3-free entry point

fn bench_numeric_conversion(c: &mut Criterion) {
    let data: Vec<f64> = (0..1_000_000).map(|i| i as f64).collect();
    c.bench_function("numeric_1M_conversion", |b| {
        b.iter(|| from_numpy_buffer(&data))
    });
}

criterion_group!(benches, bench_numeric_conversion);
criterion_main!(benches);
```

### Fixing the pyproject.toml numpy dev-pin (Pitfall 4)
```diff
 [dependency-groups]
 dev = [
     "duckdb==1.5.4",
     "hypothesis==6.156.6",
     "maturin==1.14.1",
-    "numpy==2.5.1",
+    "numpy>=2.3,<2.6",
     "pandas==3.0.3",
     "polars==1.42.1",
     "pyarrow==25.0.0",
     "pytest==9.1.1",
+    "pytest-benchmark>=5.2,<6",
+    "psutil>=7.2,<8",
 ]
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|-------------------|---------------|--------|
| PyPI API token stored as a repo secret, used with `twine upload` | OIDC trusted publishing via `pypa/gh-action-pypi-publish` (`id-token: write`, no stored secret) | PyPI trusted publishing GA'd 2023-2024, now the documented default recommendation | Eliminates long-lived-token leak/staleness risk entirely; short-lived per-run credential |
| QEMU-emulated aarch64 Linux CI builds/tests | Native `ubuntu-24.04-arm`/`ubuntu-22.04-arm` free hosted runners | GA August 2025 for public repos `[CITED: github.blog changelog]` | Faster CI, and aarch64 wheels can actually be *tested*, not just built, without the QEMU speed penalty |
| `resource.getrusage`-only memory measurement (Unix-only) | `psutil`-based cross-platform measurement | N/A — `psutil` has always been the cross-platform answer; noted here because this project's target audience includes Windows users (Windows wheel is explicitly in scope per D-34) | Windows RSS measurement has no `resource`-module equivalent at all; `psutil` is not optional for this phase's BENCH-02 if Windows wheels are being shipped |

**Deprecated/outdated:**
- Manual `twine upload` with a `PYPI_API_TOKEN` secret: still works, but no longer the recommended pattern for new projects — OIDC trusted publishing is the current default.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|-----------------|
| A1 | `uv`'s universal resolver can select different concrete numpy versions per Python-version marker within a single `uv.lock`, resolving the D-35/numpy-pin conflict without needing separate lockfiles per CI matrix cell | Pitfall 4, Code Examples | If wrong, the planner needs a fallback of maintaining separate `uv.lock` files (or `uv pip compile` invocations) per Python version in the CI matrix instead of one universal lock — more CI complexity, not a blocker, but changes the packaging task's shape |
| A2 | `macos-13` (x86_64) and `macos-14` (arm64) remain the correct GitHub-hosted runner labels for building both macOS architectures without cross-compilation, at the time the CI is actually implemented | Architecture Patterns Pattern 2 | GitHub periodically retires older macOS runner images; if `macos-13` is deprecated by execution time, the planner needs to check current available macOS runner labels and may need `macos-13-large` or similar, or fall back to `universal2` cross-compilation |
| A3 | This repository is a **public** GitHub repo, making the free native `ubuntu-24.04-arm` runner tier available | Pitfall 5 | If the repo is actually private, the free-tier native arm64 runner may not apply (private-repo arm64 GA'd January 2026 per search results but likely under different/paid billing) — the planner should verify repo visibility before assuming the free native-arm build path |
| A4 | The `pydart` PyPI-name collision (Pitfall 6) is not resolvable by simply publishing and hoping — PyPI enforces global uniqueness of package names with no override, confirmed by the registry actually returning an existing project's metadata for that exact name | Summary, Pitfall 6 | Very low risk of being wrong — this was a direct HTTP 200 registry check, not an inference; included here only because it's the single most consequential finding in this document and deserves an explicit confidence marker (this one is VERIFIED, not assumed) |

**Note:** A4 is included for completeness of the assumptions-log format but is itself `[VERIFIED]`, not `[ASSUMED]` — it is listed to make clear that the PyPI-collision finding was checked, not inferred, and does not need further user confirmation to be treated as true (though the *response* to it, listed in Open Questions, absolutely does need a user decision).

## Open Questions

1. **How should the D-32/D-33 vs. real-PyPI-name-collision conflict be resolved?**
   - What we know: `pydart` is registered on PyPI by an unrelated project (verified, HTTP 200). D-32 requires `pip install pydart` to work; D-33 locks "pydart" as the PyPI listing name.
   - What's unclear: Whether the user wants to (a) publish under an available alternate PyPI name while keeping `import pydart` as the Python module path (PyPI distribution name and importable module name are independent — very common, e.g. `pip install beautifulsoup4` -> `import bs4`), (b) pick a new name for everything and re-run the rename process, or (c) pursue a PyPI name-dispute/transfer request (slow, no guaranteed outcome, likely incompatible with this phase's timeline).
   - Recommendation: Escalate to the user before the planner writes any publish-pipeline tasks. Option (a) is the lowest-disruption path (zero code changes, only `pyproject.toml`'s `name = "..."` field and CI publish config change) and is worth proposing as the default suggestion, but this is explicitly the user's call since it revises a locked decision (D-32/D-33).

2. **What is the pass/fail bar for "measurably faster than pyarrow" if some benchmark cells show pydart is *not* faster (or is slower)?**
   - What we know: PROJECT.md frames this as existential ("must be provably faster... or the project has no reason to exist"), and STACK.md's own Critical Caveat already predicts that non-zero-copy-eligible scenarios (object/categorical/nullable) may show a smaller win or even a loss versus pyarrow's years-mature C++ copy path.
   - What's unclear: Whether "the project's core value claim is proven" requires *every* matrix cell to win, or only the true-zero-copy cells (numeric/non-null/Arrow-backed), with the copy-fallback cells allowed to be "honestly reported, not required to win."
   - Recommendation: The planner should propose a specific, falsifiable pass bar (e.g., "true zero-copy scenarios must show >= 2x throughput advantage; copy-fallback scenarios must be within +/-20% of pyarrow, reported either way") as part of Phase 4 planning, and treat this as a decision needing explicit user sign-off before benchmark results are called "success," not an implicit any-result-is-fine criterion.

3. **Exact numpy/pandas version-matrix endpoints for PKG-02 (D-37's "oldest to newest" within corrected floors)?**
   - What we know: Python floor is >=3.11 (D-35); pandas floor is >=3.0 (D-36). Registry-verified: pandas 3.0.0 is the oldest release satisfying that floor (requires_python >=3.11); pandas 3.0.5 is newest at research time (only patch releases exist in the 3.x line so far — no 3.1 minor yet). For numpy: versions 2.3.0-2.4.1 support Python >=3.11; numpy 2.5.0/2.5.1 dropped 3.11 support (requires >=3.12). Since the CI Python-version matrix itself likely spans 3.11 and 3.12+ (to match the abi3-py311 floor and test forward compatibility), the numpy "newest" endpoint differs *per Python version in the matrix* — e.g. numpy 2.4.x is the newest usable on a 3.11 job, while numpy 2.5.1 is available and newest on a 3.12+ job.
   - What's unclear: Whether the planner wants a full cross-product matrix (python x numpy x pandas) or a curated subset (e.g., {oldest-python + oldest-numpy + oldest-pandas} and {newest-python + newest-numpy + newest-pandas} as the two matrix endpoints, per common practice for compatibility-matrix testing without combinatorial blowup).
   - Recommendation: Start with the two-endpoint approach (oldest/newest combination, not full cross-product) given `mode: yolo`/`granularity: coarse` project settings favor pragmatic scope; concrete registry-verified endpoint versions: oldest = {Python 3.11, numpy 2.3.0, pandas 3.0.0}, newest = {Python 3.12+, numpy 2.5.1, pandas 3.0.5}.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| `uv` | PKG-03, dev workflow | Yes — confirmed installed at `.venv` (`pyvenv.cfg`, `uv.lock` present) | — (not directly queried; `uv.lock` exists in repo root) | — |
| GitHub Actions (`.github/workflows/`) | PKG-01, PKG-02, D-32 publish pipeline | No — directory does not exist yet, confirmed via `find` | — | None needed — this phase creates it from scratch (greenfield, no fallback required) |
| Rust toolchain (`cargo`, `rustc`) | Benchmark harness (criterion), all wheel builds | Yes — confirmed via existing `target/` build artifacts and `rust-toolchain.toml` | rust-version = "1.75" (workspace pin) | — |
| `psutil`/`pytest-benchmark`/`pytest-codspeed` | BENCH-01/BENCH-02 | Not yet installed (not in current `pyproject.toml` dev group) | Target: psutil 7.2.2, pytest-benchmark 5.2.3 | None needed — trivial `uv add --dev` install |
| Native `ubuntu-24.04-arm` GitHub-hosted runner | D-34 aarch64 Linux wheel build/test | Depends on repo visibility (public vs private) — not verified in this research session (no access to check the actual GitHub repo settings) | GA since Aug 2025 for public repos; Jan 2026 for private | QEMU cross-compilation via `docker/setup-qemu-action` if repo is private and native tier unavailable/billed differently |
| Real PyPI availability of name `pydart` | D-32 (blocking) | **No** — confirmed taken by an unrelated project | — | See Open Question 1 — requires user decision, not a technical fallback |

**Missing dependencies with no fallback:**
- The `pydart` PyPI name itself — this blocks D-32/D-33 as currently locked and requires a user decision (see Open Questions).

**Missing dependencies with fallback:**
- Native arm64 GitHub runner (fallback: QEMU cross-compile, slower but functional) — only relevant if the repo turns out to be private.

## Security Domain

`security_enforcement` is enabled (absent = enabled per config; `.planning/config.json` confirms `"security_enforcement": true`, ASVS level 1). For this phase, the relevant threat surface is **supply-chain / release-pipeline integrity**, not input validation — there is no new user-facing data-handling code in this phase.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|-----------------|---------|--------------------|
| V2 Authentication | Yes (release pipeline, not app users) | OIDC trusted publishing for PyPI (`id-token: write`), not a stored long-lived API token — treat the publish workflow itself as the "authenticating principal" |
| V5 Input Validation | No (n/a — no new user-facing input paths in this phase) | — |
| V6 Cryptography | No (n/a — no new crypto surface) | — |
| V14 Configuration | Yes | CI workflow permissions should be minimally scoped (`id-token: write` only on the publish job, not repo-wide); manylinux/glibc tag should be an explicit, deliberate choice (Pitfall 3/CLAUDE.md), not a default |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|------------------------|
| Long-lived PyPI API token leaked from CI secrets or logs | Spoofing / Elevation of Privilege | OIDC trusted publishing (short-lived, workflow-scoped credential; nothing to leak) |
| Malicious/typosquatted transitive dependency introduced via a new dev dependency (e.g. a "helper" package added casually alongside `pytest-benchmark`/`psutil`) | Tampering | Package Legitimacy Audit gate (already run above) before any new dependency is added; prefer well-known, long-established packages with public source repos |
| Dependency-confusion attack against the eventual real PyPI publish (a malicious package pre-registered under a similar/confusable name) | Spoofing | Directly relevant given the Pitfall 6 finding — whatever name is ultimately chosen to resolve the collision must itself be re-verified as unclaimed/legitimate before locking it in, not assumed available from training-data memory |
| Overly broad workflow permissions (e.g. `permissions: write-all` on the whole workflow instead of scoping `id-token: write` to just the publish job) | Elevation of Privilege | Scope `permissions:` at the job level, not the workflow level; publish job gets `id-token: write`, other jobs get default read-only |

## Sources

### Primary (HIGH confidence)
- PyPI registry API (`https://pypi.org/pypi/{pydart,numpy,pandas}/json` and specific version endpoints) — direct registry verification of the PyPI name collision, numpy 2.5.1/2.3.0-2.4.1/2.0.0/1.26.4 `requires_python` fields, pandas 3.0.0/3.0.5 `requires_python` and `numpy` requirement, fetched 2026-07-27
- crates.io registry API (`https://crates.io/api/v1/crates/criterion`) — criterion 0.8.2 version verification, fetched 2026-07-27
- GitHub Tags API (`api.github.com/repos/PyO3/maturin-action/tags`, `api.github.com/repos/pypa/gh-action-pypi-publish/tags`) — maturin-action v1.51.0, gh-action-pypi-publish v1.14.1, fetched 2026-07-27
- `pip index versions {numpy,pandas,pytest-benchmark,psutil,pytest-codspeed}` — direct local pip registry query, fetched 2026-07-27
- This project's own `.planning/research/{STACK,ARCHITECTURE,PITFALLS}.md` (init research, 2026-07-13) — HIGH-confidence prior-phase-verified findings on PyO3/arrow-rs/maturin stack, zero-copy dtype eligibility, and the Pitfall 6/7 (benchmark rigor, packaging ABI) recommendations this phase directly inherits
- `gsd-tools query package-legitimacy check` — pytest-benchmark/psutil/pytest-codspeed existence and repo-URL signals, run 2026-07-27

### Secondary (MEDIUM confidence)
- [GitHub Changelog: arm64 hosted runners for public repositories are now generally available](https://github.blog/changelog/2025-08-07-arm64-hosted-runners-for-public-repositories-are-now-generally-available/) — native aarch64 runner availability
- [GitHub Changelog: arm64 standard runners now available in private repositories](https://github.blog/changelog/2026-01-29-arm64-standard-runners-are-now-available-in-private-repositories/) — private-repo arm64 timeline
- [PyPI Docs: Publishing with a Trusted Publisher](https://docs.pypi.org/trusted-publishers/using-a-publisher/) — OIDC trusted publishing setup
- [GitHub Docs: Configuring OpenID Connect in PyPI](https://docs.github.com/en/actions/security-for-github-actions/security-hardening-your-deployments/configuring-openid-connect-in-pypi)
- [pypa/gh-action-pypi-publish GitHub repo](https://github.com/pypa/gh-action-pypi-publish)
- [PyO3/maturin-action GitHub repo](https://github.com/PyO3/maturin-action)
- [Maturin User Guide: Distribution](https://www.maturin.rs/distribution.html) — manylinux2014 glibc 2.17 minimum guidance
- [psutil documentation](https://psutil.readthedocs.io/) and [giampaolo/psutil#1096](https://github.com/giampaolo/psutil/issues/1096) — cross-platform RSS measurement, `ru_maxrss` unit inconsistency
- General web search on uv universal-resolution behavior across Python-version markers — cross-corroborated across multiple community sources, no single official uv doc page found with this exact scenario spelled out

### Tertiary (LOW confidence)
- None flagged separately — all claims above were either registry-verified or cited against an official/semi-official source; where a claim rests purely on general web search with no cross-corroboration, it is marked `[CITED: general web search]` inline rather than presented as verified.

## Metadata

**Confidence breakdown:**
- Standard stack (bench tooling, maturin-action, gh-action-pypi-publish versions): HIGH — all registry/API-verified, not recalled from training data
- Architecture (CI matrix shape, benchmark harness structure): MEDIUM — synthesized from official docs + community patterns, no single authoritative "this exact project's CI" reference exists yet since `.github/` is greenfield
- Pitfalls: HIGH for the PyPI name collision (direct registry check) and the numpy-pin resolver conflict (direct registry check); MEDIUM for benchmark-methodology specifics (inherited MEDIUM flag from this project's own init PITFALLS.md, unchanged by this session's research)

**Research date:** 2026-07-27
**Valid until:** 7 days for the PyPI-name-availability and package-version findings (registry state can change; re-check immediately before the publish step executes, not just at plan time) — 30 days for the architectural/CI-pattern guidance (stable, slower-moving domain)
