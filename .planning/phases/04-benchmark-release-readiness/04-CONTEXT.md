# Phase 4: Benchmark & Release Readiness - Context

**Gathered:** 2026-07-27
**Status:** Ready for planning

<domain>
## Phase Boundary

The project's core value claim — measurably faster than pyarrow, zero-copy where physically possible — is proven with a realistic benchmark matrix (BENCH-01, BENCH-02), and the package (now named **pydart**, not "flint" — see Decisions below) is built as installable, cross-platform wheels and actually published to PyPI, verified across platforms and a supported Python/numpy/pandas version matrix, and installable via `uv` (PKG-01, PKG-02, PKG-03). No new conversion/IO capabilities are added — this phase proves and ships what Phases 1-3 already built.

</domain>

<decisions>
## Implementation Decisions

### PyPI Publishing & Package Identity
- **D-32:** Phase 4 actually publishes the package to real PyPI — not build-and-verify-only, not TestPyPI-only. `pip install pydart` must work for real users by the end of this phase.
- **D-33:** The project is renamed from "flint" (placeholder) to **pydart**, finalized across the PyPI listing, Python import path (`import pydart`), Rust crates (`pydart-core`, `pydart-python`), the compiled extension (`pydart._pydart`), the Python-visible exception (`pydart.PydartError`), all tests, and docs. This was executed as a **prerequisite quick task** (`.planning/quick/260727-ih5-rename-the-project-from-flint-to-pydart-/`, commits `f972caf`..`ec5bea7`, merged to `master`) rather than as Phase 4's first plan task — keeps this phase's actual plan scoped to benchmarking + packaging, not the rename itself. All downstream agents (researcher, planner, executor) MUST use "pydart" as the project/package name — "flint" no longer exists in the codebase except in historical `.planning/` narrative.

### Platform, Architecture & Version Floor
- **D-34:** Wheels target **x86_64 AND aarch64/arm64** for Linux (manylinux) and macOS, plus Windows x86_64 — not x86_64-only. Matches what pyarrow/polars/duckdb already ship; an ARM-only user hitting "no wheel for your platform" is exactly the adoption friction RESEARCH.md's Pitfall 7 warns about.
- **D-35:** `requires-python` in `pyproject.toml` is corrected from the stale `>=3.12` (a leftover dev-dependency-resolution artifact, not a real floor) to **`>=3.11`**, matching the actual compiled `abi3-py311` wheel floor (see PROJECT.md Key Decisions: PyO3 abi3 floor was raised to abi3-py311 in Phase 1). Dev-dependency pins (numpy, pandas, etc.) must be re-resolved to actually work under Python 3.11, not just documented as compatible — this is a real task, not a one-line edit.
- **D-36:** The pandas floor is pinned to **`>=3.0`** (not the previously-claimed `>=2.2`) specifically to close the WR-02 blocker recorded in STATE.md: pandas' Copy-on-Write (CoW) is unconditional only from pandas 3.0 onward, and the zero-copy numpy buffer borrow's post-borrow-mutation safety relies entirely on CoW being active — CoW is off by default pre-3.0. This narrows the supported pandas range but **resolves a real correctness gap** before public release, rather than leaving it as a documented risk. `.claude/CLAUDE.md`'s "pandas >= 2.2" support claim and `pyproject.toml`'s dependency floor both need updating to reflect this new floor.
- **D-37:** The CI version matrix (PKG-02) spans oldest-to-newest supported numpy/pandas versions within the corrected floors (Python >=3.11, pandas >=3.0). The exact version list to test is Claude's/planner's discretion at plan time (see Claude's Discretion below).

### Benchmark Claim Presentation & Data Realism
- **D-38:** Benchmark results are published as a **committed `BENCHMARKS.md` in the repo** — methodology plus raw numbers, regenerated per release — not an external CI-service dashboard (e.g. CodSpeed). Keeps the "faster than pyarrow" claim versioned alongside the code with no external service dependency, per RESEARCH.md Pitfall 6's guidance to "publish the benchmark methodology and raw data alongside the claim so it survives community scrutiny."
- **D-39:** Benchmark data is **synthetic-only** for v1 — no public/real-world dataset download (e.g. no NYC taxi data). All scenarios (numeric, mixed, nullable, chunked, object-string, categorical) are generated programmatically with controlled, documented shapes/row counts. Fully reproducible, no network or licensing dependency in CI.
- **D-40:** Categorical/dictionary columns **ARE included** in the benchmark matrix, including Parquet-IO scenarios, **with an explicit caveat** in `BENCHMARKS.md`/release docs calling out the known gap from Phase 3 (STATE.md, T-03-09): categorical `.cat.categories` order and unused-category retention do NOT survive a Parquet round-trip (an arrow-rs `ArrowWriter`/`DictEncoder` limitation — values and `dict_is_ordered` DO survive correctly). This is consistent with the project's established diagnostics-honesty posture (D-03/D-04, D-26) — never hide a known limitation behind a speed claim.

### Claude's Discretion
- Exact benchmark tooling: pytest-benchmark vs CodSpeed's local/self-hosted mode vs a custom timing harness for Python-level benchmarks; `criterion` for Rust-side kernels. CLAUDE.md already recommends pytest-benchmark/codspeed + criterion; RESEARCH.md/PITFALLS.md flag exact methodology specifics as "validate at plan time" (MEDIUM confidence).
- Memory-measurement method (`tracemalloc`, `resource.getrusage`, `psutil`, or a Rust-side allocation counter) and handling cross-platform RSS-measurement quirks (Windows differs from Linux/macOS).
- manylinux glibc tag choice (`manylinux2014` vs `manylinux_2_28`) — technical detail within the locked "manylinux + arm64" decision (D-34).
- CI provider setup mechanics — GitHub Actions assumed (repo is hosted on GitHub; `maturin-action` is GitHub-specific and is CLAUDE.md's recommended tool). No `.github/workflows/` exists yet — greenfield.
- Exact numpy/pandas version list for the CI compatibility matrix (D-37) — oldest-to-newest within the corrected floors (Python >=3.11, pandas >=3.0).
- PyPI trusted-publisher (OIDC) vs classic API-token authentication for the actual publish step, and the release-tagging mechanism (git tag trigger vs manual workflow dispatch).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Research (from project init)
- `.planning/research/STACK.md` — pytest-benchmark/codspeed + criterion tooling recommendation; maturin/manylinux/abi3 packaging guidance
- `.planning/research/PITFALLS.md` — Pitfall 6 ("Benchmark claims that don't survive scrutiny") and Pitfall 7 ("Packaging/ABI failures that only surface for a subset of users' environments") directly shape D-34 through D-40
- `.planning/research/ARCHITECTURE.md` — component boundaries relevant to where benchmark harnesses (Rust `benches/`, Python-level) should live
- `.planning/research/SUMMARY.md` — synthesized findings

### Project Context
- `.planning/PROJECT.md` — core value claim ("measurably faster than pyarrow... or the project has no reason to exist"); Key Decisions table (abi3-py311 floor, "Success measured by benchmark vs pyarrow — Pending (Phase 4)", categorical Parquet fidelity gap noted for Phase 4 release docs)
- `.planning/REQUIREMENTS.md` — BENCH-01, BENCH-02, PKG-01, PKG-02, PKG-03 full requirement text
- `.planning/STATE.md` — Blockers/Concerns section: WR-02 (pandas CoW gap, resolved by D-36) and the categorical Parquet fidelity gap (T-03-09, addressed by D-40); benchmarking methodology/manylinux floor explicitly flagged as MEDIUM-confidence, task-derived recommendations to validate at plan time
- `.planning/ROADMAP.md` — Phase 4 goal, success criteria, requirements mapping
- `.claude/CLAUDE.md` — recommended stack table (pytest-benchmark/codspeed, criterion, maturin-action, manylinux/abi3/glibc guidance), "What NOT to Use" table (cross-refs Pitfall 7's packaging/ABI concerns)
- `.planning/quick/260727-ih5-rename-the-project-from-flint-to-pydart-/260727-ih5-SUMMARY.md` — the flint->pydart rename executed as a prerequisite to this phase (D-33); all downstream agents must use "pydart" as the project/package name
- `.planning/phases/03-parquet-io/03-CONTEXT.md` — Phase 3's D-19 naming-convention decisions and the T-03-09 categorical Parquet fidelity gap referenced by D-40

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `pyproject.toml` (now declares `name = "pydart"`, `module-name = "pydart._pydart"`) — the file PKG-01/02/03 work extends: add benchmark dev-dependencies (`pytest-benchmark` and/or `codspeed`), update `requires-python` (D-35) and pandas floor (D-36) here.
- Root `Cargo.toml` workspace + `crates/pydart-core/`, `crates/pydart-python/` — Rust-side benchmark kernels (`criterion`) would live as a new `benches/` target, likely in `pydart-core` since it's the pyo3-free crate — measuring pure-Rust conversion/IO kernels without PyO3/GIL overhead.
- `tests/python/*.py`, `tests/rust/*.rs` — existing test suite structure/dtype-shape fixtures (e.g. `test_categorical.py`, `test_object_string.py`, `test_nulls.py`) that benchmark scenarios can mirror for realistic column shapes.
- No CI workflows exist yet (`.github/` is empty) — Phase 4 is greenfield for CI; no prior GitHub Actions patterns to follow or conflict with.

### Established Patterns
- `pydart-core` / `pydart-python` pyo3-free-core / pyo3-bindings split (renamed from `flint-core`/`flint-python`, same architecture) — Rust-side benchmarks belong in `pydart-core` to isolate "is slowness in Rust or at the FFI/GIL boundary," per CLAUDE.md's criterion rationale.
- Named, specific errors / diagnostics-honesty pattern (D-03/D-04, DIAG-01/02, D-26) — extends naturally to D-40's benchmark-caveat requirement: never silently omit a known limitation from a public claim.

### Integration Points
- No `benches/` directory or benchmark harness exists yet — this phase creates it from scratch.
- No `.github/workflows/` exists yet — the CI matrix (PKG-02) and wheel-building pipeline (PKG-01) are both greenfield.

</code_context>

<specifics>
## Specific Ideas

- The user chose to rename the project from "flint" to "pydart" mid-discussion (triggered by the "publish to real PyPI" decision, since PROJECT.md still called "flint" a placeholder name), and explicitly asked for it to be executed as a separate `/gsd-quick` task before continuing this discussion — see `.planning/quick/260727-ih5-rename-the-project-from-flint-to-pydart-/260727-ih5-SUMMARY.md` for the full rename record (4 tasks, 141/141 tests passing after rename).
- The user is firmly aligned with the project's existing "never silently hide a known limitation" posture (D-03/D-04, D-26): chose to benchmark categorical columns WITH an explicit caveat (D-40) rather than excluding them from the matrix or benchmarking them silently.
- The user chose the narrower, more rigorous option at every real correctness-vs-convenience fork this phase raised: closing the WR-02 pandas CoW gap by pinning a floor (D-36) rather than leaving it as documented risk, and caveating the categorical gap (D-40) rather than omitting it.

</specifics>

<deferred>
## Deferred Ideas

- **External CI-service benchmark dashboard (e.g. CodSpeed)** — considered but not chosen for v1; `BENCHMARKS.md` in-repo is the canonical claim (D-38). Revisit if ongoing PR-level regression tracking becomes valuable post-release.
- **Semi-realistic public dataset (e.g. NYC taxi) for benchmarks** — deferred; synthetic-only for v1 (D-39). Revisit if community scrutiny specifically challenges the synthetic-data methodology.

None — discussion stayed within phase scope. (PyPI trusted-publisher mechanics and release-tagging strategy are left to Claude's/planner's discretion, not deferred to a future phase — still in scope for this phase, just not user-decided.)

</deferred>

---

*Phase: 4-benchmark-release-readiness*
*Context gathered: 2026-07-27*
