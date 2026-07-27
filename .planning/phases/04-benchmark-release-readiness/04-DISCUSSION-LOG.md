# Phase 4: Benchmark & Release Readiness - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-27
**Phase:** 4-benchmark-release-readiness
**Areas discussed:** PyPI publishing scope, Platform/arch & version-floor breadth, Where the speed claim lives + data realism

---

## PyPI publishing scope

| Option | Description | Selected |
|--------|-------------|----------|
| Build & verify only | CI builds and smoke-tests wheels, no publish step | |
| Publish to TestPyPI | Proves the release pipeline against the test index only | |
| Publish to real PyPI | Full release — `pip install <name>` works for real users | ✓ |

**User's choice:** Publish to real PyPI.
**Notes:** This immediately raised the question of the project's still-placeholder name.

### Follow-up: package name

| Option | Description | Selected |
|--------|-------------|----------|
| Lock in 'flint' | Confirm 'flint' as final (pending PyPI availability check) | |
| Rename now, before publishing | User names the real final name | ✓ ("pydart") |
| Reserve placeholder, decide later | Publish under a temp name or fall back to TestPyPI | |

**User's choice:** Rename to "pydart" (free-text answer).
**Notes:** Confirmed explicitly after the assistant reflected back the full scope of the rename (crate names, import path, PyPI listing, PROJECT.md title).

### Follow-up: rename execution mechanism

| Option | Description | Selected |
|--------|-------------|----------|
| As Phase 4's first plan task | Rename scheduled as the first wave of Phase 4's actual plan | |
| Separate /gsd-quick task first | Isolated, atomic rename before continuing this discussion | ✓ |

**User's choice:** Separate /gsd-quick task first.
**Notes:** Executed immediately (quick task 260727-ih5): Rust crate/workspace rename, Python package/binding rename, docs rename, full build+test verification. 4 commits, merged to master, 141/141 tests passing. See `260727-ih5-SUMMARY.md` for the full record, including a worktree-cleanup incident (a false-positive safety-gate block, manually resolved, with two doc artifacts lost to an operator error during cleanup and subsequently reconstructed).

---

## Platform/arch & version-floor breadth

| Option | Description | Selected |
|--------|-------------|----------|
| x86_64 only | Narrower CI matrix, excludes ARM | |
| x86_64 + arm64 | Matches pyarrow/polars/duckdb; broader install base | ✓ |

**User's choice:** x86_64 + arm64.

### Follow-up: Python floor

| Option | Description | Selected |
|--------|-------------|----------|
| 3.11 (matches abi3 build) | Lower requires-python to the true abi3-py311 floor | ✓ |
| Keep 3.12 | Leave the artificially narrower dev-environment constraint | |

**User's choice:** 3.11 (matches abi3 build).

### Follow-up: pandas CoW / WR-02 gap

| Option | Description | Selected |
|--------|-------------|----------|
| Pin pandas>=3.0 (CoW always on) | Closes WR-02 by construction | ✓ |
| Support pandas>=2.2, add runtime CoW check | Wider range, guarded at runtime | |
| Leave as documented risk | No change, keep as accepted gap | |

**User's choice:** Pin pandas>=3.0 (CoW always on).
**Notes:** Narrows the supported pandas range but resolves a real correctness gap rather than shipping it as a known risk.

---

## Where the speed claim lives + data realism

| Option | Description | Selected |
|--------|-------------|----------|
| BENCHMARKS.md in the repo | Committed, versioned, no external dependency | ✓ |
| External CI-service dashboard (CodSpeed) | Trend graphs, PR regression comments, external dependency | |
| Both | BENCHMARKS.md as canonical + CI dashboard for tracking | |

**User's choice:** BENCHMARKS.md in the repo.

### Follow-up: data realism

| Option | Description | Selected |
|--------|-------------|----------|
| Synthetic only | Fully reproducible, no network/licensing dependency | ✓ |
| Add a semi-realistic public dataset | More persuasive, less-controlled CI data shape | |

**User's choice:** Synthetic only (recommended for v1).

### Follow-up: categorical Parquet fidelity caveat (T-03-09)

| Option | Description | Selected |
|--------|-------------|----------|
| Benchmark categorical, caveat the gap | Include + honestly document the known limitation | ✓ |
| Benchmark categorical, no caveat needed | Include without repeating the documented gap | |
| Exclude categorical from Parquet benchmarks | Sidestep the question entirely | |

**User's choice:** Benchmark categorical, caveat the gap.

---

## Claude's Discretion

- Exact benchmark tooling (pytest-benchmark vs CodSpeed local mode vs custom harness; criterion for Rust-side kernels)
- Memory-measurement method and cross-platform RSS quirks
- manylinux glibc tag choice (manylinux2014 vs manylinux_2_28)
- CI provider setup mechanics (GitHub Actions assumed)
- Exact numpy/pandas version list for the CI compatibility matrix
- PyPI trusted-publisher (OIDC) vs API-token auth; release-tagging mechanism

## Deferred Ideas

- External CI-service benchmark dashboard (e.g. CodSpeed) — revisit if ongoing PR-level regression tracking becomes valuable post-release
- Semi-realistic public dataset (e.g. NYC taxi) for benchmarks — revisit if community scrutiny challenges the synthetic-data methodology
