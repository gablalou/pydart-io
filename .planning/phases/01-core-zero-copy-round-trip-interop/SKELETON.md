# Walking Skeleton — Flint (placeholder name)

**Phase:** 1
**Generated:** 2026-07-13

## Capability Proven End-to-End

A user can call `flint.Table.from_pandas(df)` on a non-null numeric-`ArrowDtype` pandas DataFrame, round-trip it back with `table.to_pandas()` with values preserved, AND export that same `Table` to pyarrow via the Arrow PyCapsule Interface (`__arrow_c_stream__`) — exercising the entire stack (arrow-rs core → PyO3/pyo3-arrow binding → Python `Table` API → PyCapsule handoff to a real external consumer) in one thin path.

> Mapping note: this project is a Rust-backed Python data-interop library, not a web app. "One real DB read/write" maps to the pandas↔Arrow round-trip (the actual data-layer operation); "one real UI interaction wired to the API" maps to the PyCapsule handoff to pyarrow (the user-facing interop surface); "deployment to a dev environment" maps to the documented `maturin develop` + `uv run pytest` local full-stack run command.

## Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Crate layout | Two-crate Cargo workspace: `flint-core` (pure Rust, zero PyO3 dep) + `flint-python` (the only crate depending on `pyo3` + `pyo3-arrow`) | RESEARCH.md recommends the `arro3`-style core+bindings split (Claude's Discretion per CONTEXT.md); keeps the pure-Rust conversion/allocation logic testable in isolation (required for the D-06 `allocation-counter` proof, which must run without a Python interpreter attached) |
| `Table` composition | `#[pyclass(name = "Table")]` wraps (composes) a `pyo3_arrow::PyTable` as an internal field; delegates PyCapsule dunders to it — never hand-rolls `FFI_ArrowArray`/`FFI_ArrowSchema` (RESEARCH.md Pattern 1, D-01) | pyo3-arrow already implements safe PyCapsule export/import; hand-rolling reintroduces the double-free/leak bug class (PITFALLS.md Pitfall 1) |
| pandas-boundary decision logic | Lives in `crates/flint-python/src/pandas.rs` as a single per-column `plan_column` function; both `from_pandas`/`to_pandas` and the strict-mode/`copy_report()` surface consume it (RESEARCH.md Pattern 3 / Pitfall 2) | One source of truth prevents strict mode (DIAG-01) and diagnostics (DIAG-02) from silently disagreeing; per-column granularity avoids pyarrow's broken table-level `zero_copy_only` failure mode |
| Strict-mode / diagnostics design | `from_pandas(df, strict=False)` keyword; strict raises `flint.ZeroCopyRequiredError` (subclass of `flint.FlintError`) naming column + dtype + reason (D-03); `table.copy_report()` returns `list[ColumnCopyStatus]` frozen dataclass (D-04) | Exact exception hierarchy and report shape are Claude's Discretion per CONTEXT.md; these follow the RESEARCH.md Code Examples recommendation |
| Public naming | Class `Table`; methods `from_pandas` / `to_pandas` mirroring pyarrow exactly (D-01, D-02) | Migrating pyarrow users can often change only the import |
| Build / dev workflow | `maturin` build backend via `pyproject.toml`; `uv`-managed dev environment (`uv add --dev ...`, `uv run pytest`); `maturin develop` for local builds | PROJECT.md constraint: packaging/dev workflow must be uv-compatible |

## Stack Touched in Phase 1

- [x] Project scaffold — two-crate workspace, `pyproject.toml`, `uv` dev deps, lint/test runner (`cargo test`, `uv run pytest`) — Plan 01
- [x] "Routing" (module entry) — `flint-python/src/lib.rs` `#[pymodule]` exposing `Table`; `python/flint/__init__.py` re-exports — Plan 01
- [x] "Database" (data layer = pandas↔Arrow round-trip) — one real `from_pandas` read AND one real `to_pandas` write, numeric happy path — Plan 01
- [x] "UI" (interop surface) — `Table.__arrow_c_stream__` export accepted by pyarrow (one real external-consumer handoff) — Plan 01
- [x] "Deployment" (documented local full-stack run) — `maturin develop && uv run pytest` documented and exercised — Plan 01

## Out of Scope (Deferred to Later Slices)

- Nulls, object/string, categorical, datetime/timezone, timedelta, multi-chunk (ChunkedArray) columns — **Phase 2** (CONV-03..CONV-08)
- Parquet IO — **Phase 3** (PARQ-01..PARQ-06)
- Benchmarking vs pyarrow and cross-platform wheel packaging — **Phase 4** (BENCH/PKG)
- Free-threaded (`abi3t`) build track — deferred past v1 per STACK.md
- Remote/object-store Parquet, Arrow Flight, compute kernels — out of scope for v1/v2 per REQUIREMENTS.md

## Subsequent Slice Plan (within Phase 1, on top of the skeleton)

- **Plan 01** (this skeleton): thin numeric round-trip + one pyarrow PyCapsule export, full stack buildable and importable.
- **Plan 02**: generalize the pandas boundary into the full per-column `plan_column` decision matrix (numpy-numeric borrow, `ArrowDtype` bool zero-copy, numpy bool strict-rejected), plus strict mode + `copy_report()`.
- **Plan 03**: prove the zero-copy claim both ways (Python pointer-identity + Rust allocation-counter).
- **Plan 04**: PyCapsule interop — import foreign objects (CAP-02) and validate export/import against pyarrow, Polars, and DuckDB (D-05).

## Subsequent Phase Plan (later milestone phases build on these architectural decisions unchanged)

- Phase 2: full dtype & structural coverage (extends the same `plan_column` pipeline).
- Phase 3: Parquet IO against the completed Arrow core.
- Phase 4: benchmark the core value claim vs pyarrow + ship cross-platform uv-compatible wheels.
