# Phase 1: Core Zero-Copy Round-Trip & Interop - Research

**Researched:** 2026-07-13
**Domain:** Rust/PyO3/arrow-rs Arrow<->pandas conversion + Arrow PyCapsule Interface interop (foundational FFI phase)
**Confidence:** MEDIUM (pyo3-arrow API structure and pandas zero-copy mechanics: MEDIUM, cross-corroborated across docs.rs/GitHub issues; DuckDB PyCapsule-native consumption status: LOW/ASSUMED, conflicting signals found this session — see Open Questions)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** The main Python-facing class is named `Table`, matching pyarrow's `pa.Table` convention — minimizes friction for the pyarrow-familiar audience this project targets.
- **D-02:** Conversion methods mirror pyarrow's naming exactly: `from_pandas` / `to_pandas` — a migrating user can often just change the import.
- **D-03:** Strict zero-copy mode raises a clear exception naming the offending column/dtype when a copy would be required, rather than silently succeeding or falling back — deliberately avoiding pyarrow's `zero_copy_only` credibility gap (a flag that's documented as rarely working even when it should).
- **D-04:** Per-column copy diagnostics are exposed via a separate, on-demand call (e.g. `table.copy_report()`), not bundled into the normal conversion return value — keeps the standard conversion path uncluttered while still making "why did this copy?" answerable.
- **D-05:** Phase 1's PyCapsule interop (`__arrow_c_array__`/`__arrow_c_stream__`/`__arrow_c_schema__`) must be validated against pyarrow, Polars, AND DuckDB — not just pyarrow. This directly tests the ecosystem-interop claim (CAP-01/CAP-02) rather than deferring it.
- **D-06:** Zero-copy claims (CONV-01, CONV-02) must be proven with BOTH a pointer/buffer-address identity check (same underlying memory address before/after conversion) AND an allocation-counting test (no new heap allocation for the data buffer during conversion). This is the strongest available proof and directly shapes the acceptance criteria/test suite for this phase.

### Claude's Discretion

- Crate layout (single Rust crate with a `python` feature vs. a two-crate workspace mirroring `arro3`'s core+bindings split) — a technical architecture decision, not a user-facing one. Research recommends the two-crate shape; planner should follow that unless a concrete reason emerges not to.
- Exact exception type/hierarchy for strict-mode failures, and the exact shape of the `copy_report()` return value (dict vs. dataclass vs. named object) — implementation detail within the locked decisions above.

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within Phase 1 scope. (Nulls, strings, categoricals, datetime/timezone, timedelta, and multi-chunk support are already scoped to Phase 2 per ROADMAP.md, not raised as new scope here.)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CONV-01 | User can convert a pandas DataFrame with non-null numeric/bool columns to an Arrow Table with true zero-copy (no data duplication) | See "The bool zero-copy trap" pitfall below — numeric is genuinely zero-copy via buffer-protocol borrow; **numpy-backed bool is not** (1-byte vs 1-bit packing). Resolution recommendation in Open Questions/Assumptions Log shapes which bool representation this requirement's acceptance fixture must use. |
| CONV-02 | User can convert numeric/bool Arrow Table columns back to a pandas DataFrame with true zero-copy | Same caveat in reverse (Flow 3 in ARCHITECTURE.md); see Code Examples for the buffer-protocol-view pattern and Pitfalls for the null/bool boundary. |
| DIAG-01 | User can request a strict zero-copy mode that errors instead of silently falling back to a copy | See "Strict-mode granularity" pitfall — pyarrow's table-level `zero_copy_only` is documented as broken; recommend column-level strict-mode checks. Exception hierarchy sketch in Code Examples. |
| DIAG-02 | User can query per-column diagnostics explaining whether a copy occurred and why | `copy_report()` shape recommendation in Code Examples, informed by pyarrow's/arro3's absence of an equivalent (original design surface, not copied from prior art). |
| CAP-01 | User can export a Table via the Arrow PyCapsule Interface for zero-copy handoff to pyarrow, Polars, DuckDB | pyo3-arrow struct-to-dunder mapping (Standard Stack / Architecture Patterns) shows this is largely a delegation to `pyo3_arrow::PyTable`/`PySchema`, not new marshalling code. DuckDB-specific validation approach documented; native-capsule-consumption status flagged as Open Question. |
| CAP-02 | User can import a foreign Arrow object (pyarrow Table, Polars DataFrame) via the PyCapsule Interface into a Table with zero-copy | pyo3-arrow's `FromPyObject` impls accept any PyCapsule-protocol-compliant object; see Code Examples for composition pattern and Security Domain for untrusted-capsule validation requirements. |

</phase_requirements>

## Summary

This phase is the foundational FFI slice: one non-null numeric/bool round-trip, proven zero-copy two independent ways, plus PyCapsule interop validated against three real consumer libraries. The project-level research (STACK.md/ARCHITECTURE.md/PITFALLS.md) already locked the stack (arrow-rs + PyO3 + pyo3-arrow + maturin) and the two-boundary mental model (Arrow<->Arrow is cleanly zero-copy via PyCapsule; pandas<->Arrow is copy-sometimes). This document goes one level deeper: which `pyo3-arrow` methods the planner gets "for free" versus what must be hand-written, how to concretely validate against pyarrow/Polars/DuckDB, how to implement the dual zero-copy proof, and — most importantly — a genuine tension inside this phase's own success criteria around bool.

**The single most planning-relevant finding:** Success Criterion 2 requires strict mode to *succeed* on a "non-null numeric/bool" DataFrame, but numpy packs `bool` at 1 byte/element while Arrow packs it at 1 bit/element — converting a numpy-backed bool column to Arrow is structurally a bit-packing copy, not a reinterpret-cast, no matter what FFI tooling is used. This is not a hypothetical edge case; it directly determines what test fixture proves CONV-01/CONV-02 and what strict mode must reject. Recommended resolution: the "succeeds" fixture for bool must be a `pandas.ArrowDtype`-backed column (`"bool[pyarrow]"`), which is already Arrow's own bitmap layout in memory and is therefore genuinely zero-copy; a default numpy-backed `bool` column must be a **documented, strict-mode-rejected** case with a clear per-column error, not silently downgraded to "close enough." This keeps D-03's "strict mode is a real contract" promise intact and keeps CONV-01/02's zero-copy claim honest.

The second key finding is a "free vs. hand-rolled" breakdown for the planner: `pyo3-arrow` gives CAP-01/CAP-02 almost entirely for free (export/import dunders are a delegation, not new marshalling) if the project's `Table` composes a `pyo3_arrow::PyTable`/arrow-rs `RecordBatch` internally rather than reimplementing FFI. The genuinely new, hand-written work in this phase is (a) the `Table` `#[pyclass]` shell itself, (b) the pandas-boundary decision logic (`from_pandas`/`to_pandas`), and (c) the diagnostics/strict-mode/`copy_report()` surface — confirmed original design territory because `arro3`'s own `Table` (the closest prior art, verified directly against its docs this session) has no pandas interop and no `copy_report`-equivalent at all.

**Primary recommendation:** Build `Table` as a thin wrapper composing `pyo3_arrow::PyTable` (delegate `__arrow_c_stream__`/`__arrow_c_schema__`/import `FromPyObject` to it) plus a hand-written `pandas.rs`-equivalent module that (1) detects `ArrowDtype`-backed vs. numpy-backed columns per-column, (2) borrows numpy buffers via the buffer protocol for the genuinely zero-copy numeric/ArrowDtype-bool case, (3) raises a named, column/dtype-specific exception in strict mode for anything else (including numpy-backed bool), and (4) exposes that same per-column decision data via `copy_report()`. Prove zero-copy with both a `ctypes.data`-based pointer-identity test (proves buffer sharing) and a Rust-side `allocation-counter` test (proves the Rust core made no heap allocation for the data buffer) — the two tests are complementary, not redundant, and neither alone is sufficient proof.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Arrow in-memory representation (arrays, buffers, schema) | Rust Core (arrow-rs) | — | Owned entirely by `arrow-rs`; this project does not reimplement Arrow itself |
| PyCapsule export (`__arrow_c_stream__`/`__arrow_c_schema__`/`__arrow_c_array__`) | PyO3 Binding Layer | Rust Core | Delegated to `pyo3-arrow`'s existing implementations on `PyTable`/`PySchema`/`PyArray`; the binding layer composes, not reimplements |
| PyCapsule import (foreign pyarrow/Polars/DuckDB objects -> `Table`) | PyO3 Binding Layer | — | `pyo3-arrow`'s `FromPyObject` impls do the marshalling; binding layer wraps the result in this project's `Table` type |
| pandas<->Arrow copy-vs-borrow decision (`from_pandas`/`to_pandas`) | PyO3 Binding Layer | Python-facing API (surfacing errors) | New, bespoke logic per ARCHITECTURE.md — not provided by `pyo3-arrow` or arrow-rs; must walk pandas' BlockManager per column |
| Strict zero-copy mode / exception raising | PyO3 Binding Layer | Python-facing API | Decision logic lives in Rust (dtype/contiguity checks); the exception itself must be a clean, catchable Python type at the API surface |
| `copy_report()` diagnostics | PyO3 Binding Layer | Python-facing API | Same underlying per-column decision data as strict mode, exposed via a separate on-demand call per D-04 |
| Zero-copy proof (pointer identity, allocation counting) | Test Infrastructure (cross-cutting) | Rust Core / Python-facing API | Not a runtime component — a verification harness spanning both the Rust allocator boundary and the Python/numpy buffer boundary |
| Ecosystem interop validation (pyarrow/Polars/DuckDB round-trip) | External Consumer Libraries | Python-facing API (produces/accepts the PyCapsule object) | These libraries are dev/test dependencies only, never a runtime dependency of the shipped package |

## Standard Stack

Core Rust/PyO3/arrow-rs/maturin stack versions are already verified in `.planning/research/STACK.md` — not re-derived here. This phase adds the **dev/test-only** Python dependencies needed to validate CAP-01/CAP-02 against real consumer libraries and to write the zero-copy proof suite.

### Core (already locked — see STACK.md)
| Library | Version | Purpose |
|---------|---------|---------|
| pyo3 | 0.29.0 | Rust<->Python FFI |
| pyo3-arrow | 0.19.0 | Arrow<->PyO3 conversion + PyCapsule protocol implementation |
| arrow (arrow-rs) | 59.1.0 | Rust Arrow columnar format |
| maturin | 1.14.1 | Build backend / wheel packaging |

### Supporting (new for this phase — dev/test dependencies, NOT runtime deps of the shipped package)
| Library | Version (verified this session via `pip index versions`) | Purpose | Why |
|---------|---------|---------|-----|
| pandas | 3.0.3 (current PyPI; supports both numpy-backed and `ArrowDtype`-backed columns) | The conversion-source library CONV-01/02 targets | Required to exercise `from_pandas`/`to_pandas`; confirmed via `pip index versions` this session — long release history (0.1 in ~2011 through 3.0.3), not a new/suspicious package despite an unrelated tool flag (see Package Legitimacy Audit) |
| pyarrow | 25.0.0 (current PyPI) | Primary interop consumer for CAP-01/CAP-02 validation (D-05); dev/test dependency only | Ecosystem-standard Arrow implementation; also the object CAP-02's "pyarrow Table" import path must accept |
| polars | 1.42.1 (current PyPI) | Second interop consumer for CAP-01/CAP-02 validation (D-05) | Polars >=1.3 implements the Arrow PyCapsule Interface natively — the reference "Rust+Python+Arrow, fast" sibling project per ARCHITECTURE.md |
| duckdb | 1.5.4 (current PyPI) | Third interop consumer for CAP-01/CAP-02 validation (D-05) | Explicitly locked by D-05; see Open Questions for current-version PyCapsule-native-consumption verification needed at execution time |
| numpy | 2.5.1 (current PyPI) | Buffer-protocol access from the Python test side; `ctypes.data` pointer-identity checks | Needed both as pandas' own dependency and directly for the D-06 pointer-identity proof |
| pytest | 9.1.1 (current PyPI) | Python-side test runner | Standard, already locked in STACK.md's Supporting Libraries |
| hypothesis | 6.156.6 (current PyPI) | Property-based round-trip testing | Standard, already locked in STACK.md's Supporting Libraries; useful even in this narrow numeric/bool phase for offset/stride edge cases |
| `allocation-counter` (crates.io, fornwall) | Latest (0.x) | Rust-side allocation-counting for the D-06 no-heap-allocation proof | Purpose-built for exactly this: wraps the global allocator with a counting shim; `measure()` returns `count_total` to assert against zero. See Code Examples. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `allocation-counter` crate for allocation proof | Hand-rolled custom `#[global_allocator]` with an `AtomicUsize` counter | `allocation-counter` already solves this (thread-local counting, `measure()` API) — hand-rolling duplicates a solved problem for no benefit in this narrow test-only use case |
| `ctypes.data` for pointer identity | `numpy.shares_memory()` / `numpy.may_share_memory()` | `ctypes.data` gives an exact address for a direct before/after equality assertion (simpler to reason about in a test); `shares_memory()` is more robust for detecting *partial* overlap (slicing) but is unnecessary complexity for this phase's non-null, non-sliced happy path — worth using in Phase 2 if slicing/offset cases are added |
| pandas `ArrowDtype` bool for the "strict mode succeeds" fixture | Accepting numpy-backed bool as "close enough" and copying silently under strict mode | Silently copying under a "strict" mode is exactly pyarrow's `zero_copy_only` credibility failure (PITFALLS.md Pitfall 5) that D-03 explicitly rejects — not a viable alternative, listed here to make the rejection explicit |

**Installation (dev/test dependencies for this phase, uv-compatible per project constraint):**
```bash
uv add --dev pandas pyarrow polars duckdb numpy pytest hypothesis
cargo add allocation-counter --dev  # or a Rust workspace dev-dependency, gated behind #[cfg(test)]
```

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| pandas | PyPI | 15+ yrs (releases back to 0.1, ~2011, per `pip index versions`) | Not machine-readable via this session's tooling | none reported by tool, but pandas.pydata.org / github.com/pandas-dev/pandas is the known canonical repo | SUS (tool) -> **Approved** | Tool flagged `too-new`/`unknown-downloads`; false positive — the legitimacy-check tool's `publishedAt` reflects the *latest release* timestamp, not package origin. `pip index versions pandas` shows 100+ historical releases back to 2011. Foundational, industry-standard package. |
| pyarrow | PyPI | 10+ yrs (releases back to 0.9.0) | Not machine-readable | `https://arrow.apache.org/` (Apache Software Foundation) | SUS (tool) -> **Approved** | Same false-positive pattern (`too-new`); already the project's own dev/test/benchmark dependency per STACK.md, official Apache project. |
| polars | PyPI | 5+ yrs (releases back to 0.0.1) | Not machine-readable | `https://www.pola.rs/` | SUS (tool) -> **Approved** | Same false-positive pattern; well-known, actively maintained, official org-owned repo (pola-rs/polars). |
| duckdb | PyPI | 6+ yrs (releases back to 0.0.0/0.0.2) | Not machine-readable | `https://github.com/duckdb/duckdb-python` | SUS (tool) -> **Approved** | Same false-positive pattern; official DuckDB Labs project. |
| numpy | PyPI | 15+ yrs (releases back to 1.3.0) | Not machine-readable | none reported by tool, but numpy.org / github.com/numpy/numpy is canonical | SUS (tool) -> **Approved** | Same false-positive pattern; foundational scientific-Python package. |
| pytest | PyPI | 10+ yrs (releases back to 2.0.0) | Not machine-readable | `https://github.com/pytest-dev/pytest` | SUS (tool) -> **Approved** | Same false-positive pattern; already locked in STACK.md. |
| hypothesis | PyPI | 10+ yrs (releases back to 0.0.1) | Not machine-readable | none reported by tool | SUS (tool) -> **Approved** | Same false-positive pattern; already locked in STACK.md. |
| pyo3-arrow | crates.io | 2+ yrs (first published 2024-06-25) | ~18,542/week | `https://github.com/kylebarron/arro3` | OK | Approved — already the project's core stack pick |
| numpy (rust-numpy crate) | crates.io | 9 yrs (first published 2017) | ~580,520/week | `https://github.com/PyO3/rust-numpy` | OK | Approved |
| pyo3 | crates.io | 9 yrs | ~3,633,477/week | `https://github.com/pyo3/pyo3` | OK | Approved |
| arrow | crates.io | 8 yrs | ~1,161,557/week | `https://github.com/apache/arrow-rs` | OK | Approved |
| thiserror | crates.io | 7 yrs | ~22,443,553/week | `https://github.com/dtolnay/thiserror` | OK | Approved |

**Packages removed due to `[SLOP]` verdict:** none.
**Packages flagged as suspicious `[SUS]`:** All seven PyPI dev/test dependencies above were initially flagged `SUS` by the automated legitimacy-check tool. This is documented as a **tool heuristic false-positive** (the tool's "too-new"/"unknown-downloads" signals derive from the `publishedAt` field reflecting each package's *most recent* release timestamp, not its original publication date — every one of these packages has 50-300+ historical releases stretching back 5-15+ years, confirmed directly via `pip index versions <pkg>` this session, a primary-source registry check). Given this is a confirmed false positive with primary-source evidence (not just training-data recall), no `checkpoint:human-verify` gate is recommended for these specific packages; the planner may install them directly. **All are dev/test-only dependencies — none become a runtime dependency of the shipped `flint` package**, consistent with PROJECT.md's "leaner than pyarrow" positioning and STACK.md's explicit guidance to keep pyarrow as dev-only.

## Architecture Patterns

*(High-level system architecture and boundary model are already documented in `.planning/research/ARCHITECTURE.md` — not repeated here. This section covers only what's new/needed at Phase 1 planning granularity.)*

### Recommended Project Structure (Phase 1 slice)
```
flint/
├── Cargo.toml
├── crates/
│   ├── flint-core/                # pure Rust, zero PyO3 dep
│   │   └── src/
│   │       └── table.rs           # thin RecordBatch/Table-equivalent, or re-export arrow-rs types directly for Phase 1
│   └── flint-python/               # the only crate depending on pyo3 + pyo3-arrow
│       └── src/
│           ├── table.rs           # PyTable-equivalent #[pyclass] Table; COMPOSES pyo3_arrow::PyTable
│           ├── pandas.rs           # from_pandas/to_pandas + copy-vs-borrow decision (Phase 1: numeric/bool only)
│           ├── diagnostics.rs      # copy_report() + strict-mode exception type(s)
│           └── lib.rs
├── python/flint/__init__.py
├── pyproject.toml                  # maturin build backend
└── tests/
    ├── rust/
    │   └── zero_copy_alloc.rs     # allocation-counter-based no-heap-allocation proof (D-06b)
    └── python/
        ├── test_round_trip.py     # pandas -> Table -> pandas correctness
        ├── test_zero_copy_pointer.py  # ctypes.data pointer-identity proof (D-06a)
        ├── test_strict_mode.py    # DIAG-01: strict mode succeeds (ArrowDtype bool) / rejects (numpy bool)
        ├── test_copy_report.py    # DIAG-02
        └── test_interop.py        # CAP-01/CAP-02 against pyarrow, polars, duckdb (D-05)
```

### Pattern 1: Compose `pyo3_arrow::PyTable`, don't reimplement PyCapsule marshalling
**What:** This project's `Table` `#[pyclass]` holds a `pyo3_arrow::PyTable` (or the equivalent arrow-rs `RecordBatch`/`Arc<dyn Array>` + `FieldRef` that `pyo3-arrow` already wraps) as an internal field, and delegates `__arrow_c_stream__`/`__arrow_c_schema__` to it directly rather than hand-writing `FFI_ArrowArray`/`FFI_ArrowSchema` construction.
**When to use:** Always, for CAP-01 (export). This is essentially free — confirmed via docs.rs that `PyTable`/`PyChunkedArray`/`PyRecordBatchReader` already implement `__arrow_c_stream__`, and `PySchema`/`PyField` implement `__arrow_c_schema__` (`PySchema` requires struct-type field, unpacking children — relevant since a `Table`'s schema is exported this way).
**Trade-offs:** None significant for Phase 1 — the only cost is an extra composition layer (`Table` wraps `PyTable`) versus inheriting directly, which is a minor Rust ergonomics choice, not a functional one.

### Pattern 2: `FromPyObject` composition for CAP-02 import
**What:** For importing a foreign object (pyarrow Table, Polars DataFrame) via PyCapsule, accept it as a `pyo3_arrow::PyTable` argument in the Rust function signature (or call `pyo3_arrow`'s conversion path explicitly) and wrap the result in this project's `Table` — `pyo3-arrow`'s `FromPyObject` impls already handle detecting and consuming the `__arrow_c_stream__`/`__arrow_c_array__` capsule protocol from any compliant foreign object.
**When to use:** Always, for CAP-02. Confirmed via web research this session that `pyo3-arrow`'s import path captures the exported FFI struct pointers into a Rust-owned wrapper with no data copy.
**Trade-offs:** Must still validate the foreign capsule's contents before trusting it (see Security Domain) — `pyo3-arrow` handles the *mechanical* FFI marshalling safely, but this project's own code is still responsible for not blindly trusting schema/pointer consistency from an arbitrary caller-supplied object claiming protocol compliance.

### Pattern 3: Per-column dtype-backend detection before any pandas conversion
**What:** Before converting a DataFrame column, inspect its dtype: is it `pandas.ArrowDtype`-backed (`isinstance(dtype, pd.ArrowDtype)`, or check `dtype.name.endswith("[pyarrow]")`) versus default numpy-backed? This single branch point determines the entire rest of the conversion path for that column (buffer-protocol borrow vs. copy vs. strict-mode rejection).
**When to use:** Every column, every call to `from_pandas`/`to_pandas`, and every call to `copy_report()` (the report is exactly this decision, made visible).
**Example (sketch, Rust side receiving column metadata from Python):**
```rust
// Source: derived from PITFALLS.md Pitfall 3 + ARCHITECTURE.md Flow 2, applied at Phase 1's
// numeric/bool-only scope. Not a pyo3-arrow API call — this is the bespoke decision logic
// this phase must write.
enum ColumnPlan {
    ZeroCopyBorrow,           // numeric (int/float), non-null, contiguous numpy buffer
                              // OR any ArrowDtype-backed numeric/bool column
    RequiresCopy { reason: String }, // numpy-backed bool (bit-packing), or anything
                              // outside Phase 1 scope if it reaches this path
}

fn plan_column(dtype_backend: DtypeBackend, arrow_kind: ArrowKind, is_contiguous: bool) -> ColumnPlan {
    match (dtype_backend, arrow_kind) {
        (DtypeBackend::Arrow, ArrowKind::Numeric | ArrowKind::Bool) => ColumnPlan::ZeroCopyBorrow,
        (DtypeBackend::Numpy, ArrowKind::Numeric) if is_contiguous => ColumnPlan::ZeroCopyBorrow,
        (DtypeBackend::Numpy, ArrowKind::Bool) => ColumnPlan::RequiresCopy {
            reason: "numpy bool is 1 byte/element; Arrow bool is 1 bit/element (bit-packing copy required)".into(),
        },
        _ => ColumnPlan::RequiresCopy { reason: "non-contiguous or unsupported layout".into() },
    }
}
```

### Anti-Patterns to Avoid
- **Treating "bool" as one uniform case:** As above — silently normalizing numpy-backed bool into the same "zero-copy succeeded" bucket as `ArrowDtype`-backed bool breaks the strict-mode contract (D-03) the moment a real user passes a default `pd.Series([True, False])`.
- **Checking strict-mode eligibility at the whole-Table granularity:** pyarrow's own `zero_copy_only=True` on `Table.to_pandas()` is documented as essentially never succeeding (`apache/arrow#39194`) because the check doesn't decompose per column. Always evaluate and report per-column.
- **Calling a foreign object's `__arrow_c_stream__()` twice without caching the result:** DuckDB's own relation objects are documented (`duckdb/duckdb#17084`) to error on a second call to `__arrow_c_stream__()` after the first consumes the stream — when writing the CAP-02 import path and its interop tests, consume the capsule/stream exactly once and cache the resulting `Table`, don't re-invoke the dunder speculatively.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Arrow PyCapsule export/import marshalling (`FFI_ArrowArray`/`FFI_ArrowSchema` construction, capsule destructors) | Custom `unsafe` PyCapsule creation code | `pyo3-arrow`'s `PyTable`/`PySchema`/`PyArray` (compose, delegate) | This is exactly pyo3-arrow's purpose; hand-rolling reintroduces the double-free/leak class of bug PITFALLS.md Pitfall 1 already warns about |
| numpy buffer-protocol borrowing for numeric columns | Custom raw-pointer capture from a numpy array | `rust-numpy` crate (already locked in STACK.md) | Provides safe, GIL-aware buffer access; avoids the "who owns this pointer" hazard when the Python object could be GC'd |
| Zero-copy proof harness | A single ad-hoc `assert` on timing/memory (e.g. "it ran fast") | `allocation-counter` (Rust) + `ctypes.data` identity check (Python), used together | Timing is not proof of zero-copy; these two tools directly measure the two things that actually define zero-copy (no new heap allocation; same physical buffer) |
| Rust panics/errors crossing into Python | Manual `PyErr::new` calls scattered through conversion code | `thiserror` + a single `impl From<FlintError> for PyErr` at the FFI boundary (already locked in STACK.md) | Centralizes the strict-mode/diagnostics exception mapping in one place, makes D-03's "clear exception naming the offending column/dtype" easy to guarantee consistently |

**Key insight:** Everything on the Arrow<->Arrow side of this phase (CAP-01/CAP-02) is close to "compose an existing, purpose-built crate correctly." Everything on the pandas<->Arrow side (CONV-01/CONV-02, DIAG-01/DIAG-02) is genuinely new code with no off-the-shelf solution — that asymmetry should directly shape how much task-planning effort/review scrutiny goes to each half of this phase.

## Common Pitfalls

### Pitfall 1: The bool zero-copy trap (CRITICAL — directly affects Success Criterion 2 and CONV-01/CONV-02 acceptance tests)

**What goes wrong:** A test fixture or implementation assumes "non-null numeric/bool" is one uniform zero-copy-eligible case. It is not: numpy stores `bool` as 1 byte per element; Arrow's canonical bool layout is 1 bit per element (bit-packed). Converting a default numpy-backed `pd.Series([True, False], dtype=bool)` to an Arrow boolean array is **structurally a repacking copy**, not a reinterpret-cast, regardless of FFI tooling quality. If Phase 1's "strict mode succeeds on non-null numeric/bool" acceptance test uses a numpy-backed bool fixture, either the test is wrong (strict mode should reject it) or the implementation is wrong (it's not actually zero-copy and strict mode is lying).

**Why it happens:** "Numeric/bool" reads as one homogeneous category in the phase description and success criteria, but bool's storage format diverges from int/float at exactly the point that matters for this project's core claim.

**How to avoid:** Resolve this explicitly during planning (see Assumptions Log entry A1): use `pandas.ArrowDtype`-backed bool (`pd.array([True, False], dtype="bool[pyarrow]")`) as the fixture that must succeed under strict mode — this is already Arrow's bitmap layout in pandas' own memory, so it is genuinely zero-copy in both directions. Treat default numpy-backed bool as an explicit, tested, strict-mode-rejected case with a clear error naming the column and dtype (per D-03), not a silent copy and not a test that's skipped.

**Warning signs:** A `from_pandas`/`to_pandas` implementation that doesn't special-case bool at all (treats it identically to int64); a strict-mode test suite with no numpy-backed-bool rejection test.

**Phase to address:** This phase (Phase 1) — the resolution must be locked before the acceptance test fixtures are written, since it changes what "success" means for two of the six requirements this phase covers.

---

### Pitfall 2: Strict-mode granularity (table-level vs. column-level)

**What goes wrong:** Implementing DIAG-01's strict mode as a single whole-DataFrame check (convert everything, then fail if *anything* wasn't zero-copy) rather than a per-column check produces exactly the failure mode pyarrow's own `zero_copy_only=True` is documented to have on `Table.to_pandas()` — it becomes unpredictable and, per `apache/arrow#39194`, is reported to essentially never succeed even on inputs that should qualify.

**Why it happens:** It's simpler to write one boolean gate around the whole conversion function than to plumb a per-column decision result back out.

**How to avoid:** Make the eligibility check a per-column function (see Pattern 3's `plan_column` sketch) that both strict mode and `copy_report()` call — this also means DIAG-01 and DIAG-02 share one source of truth, which keeps them from silently disagreeing.

**Warning signs:** Strict mode implemented as `try { convert_whole_table() } catch { raise }` rather than a pre-flight per-column plan.

**Phase to address:** This phase.

---

### Pitfall 3: Consuming a foreign PyCapsule stream more than once

**What goes wrong:** DuckDB's own relation objects are documented (GitHub issue `duckdb/duckdb#17084`, filed against current DuckDB Python) to raise `Invalid Input Error: There is no query result` on a *second* call to `__arrow_c_stream__()` on the same object, even though the object is otherwise still valid. If this project's CAP-02 import path (or its interop test suite) calls a foreign object's capsule dunder more than once expecting idempotency, it will intermittently fail specifically against DuckDB, while working fine against pyarrow/Polars (whose objects are documented to tolerate repeated calls).

**Why it happens:** The Arrow PyCapsule Interface protocol doesn't mandate that streams be re-consumable, and most producers (pyarrow, Polars) happen to be, masking non-idempotent producers like DuckDB relations during development against the "easy" libraries.

**How to avoid:** Consume the capsule/stream exactly once per import, immediately materializing or wrapping the result; never call the foreign dunder speculatively "to check" and then again "for real." Write the DuckDB interop test (D-05) to call the capsule method exactly once.

**Warning signs:** Interop tests that pass against pyarrow/Polars but intermittently fail only against DuckDB.

**Phase to address:** This phase, specifically the CAP-01/CAP-02 interop validation tasks.

---

### Pitfall 4: Allocation-counter false negatives from compiler optimization

**What goes wrong:** A Rust test using `allocation-counter::measure()` reports zero allocations, but not because the conversion path is actually zero-copy — because LLVM detected the measured value was unused and optimized away or stack-promoted an allocation that would have happened in production use.

**Why it happens:** `allocation-counter`'s own documentation flags this: you must not rely on allocations happening if the closure's result isn't used, since the optimizer can eliminate genuinely-unreachable-in-practice work.

**How to avoid:** Ensure the measured closure actually returns/uses the converted `Table`/array in a way the optimizer cannot prove is dead (e.g. return it from the function under test, or use `std::hint::black_box` around the result) so a genuine zero vs. non-zero allocation count is meaningful.

**Warning signs:** An allocation test that passes even when you deliberately introduce a `.clone()` or `.to_vec()` into the conversion path as a sanity check (if the test still reports zero allocations after that change, the test itself is broken, not the code).

**Phase to address:** This phase, when writing the D-06 allocation-counting test.

## Runtime State Inventory

Not applicable — this is a greenfield phase (no existing code, no rename/refactor/migration). Skipped per instructions.

## Code Examples

### `Table` composing `pyo3_arrow::PyTable` for CAP-01 export
```rust
// Source: derived from pyo3-arrow docs.rs struct-to-dunder mapping (PyTable implements
// __arrow_c_stream__; PySchema implements __arrow_c_schema__), verified this session.
use pyo3::prelude::*;
use pyo3_arrow::PyTable;

#[pyclass(name = "Table")]
struct Table {
    inner: PyTable,
}

#[pymethods]
impl Table {
    fn __arrow_c_stream__(&self, py: Python, requested_schema: Option<PyObject>) -> PyResult<PyObject> {
        // Delegate directly — pyo3-arrow already implements the correct FFI_ArrowArrayStream
        // construction and PyCapsule destructor wiring.
        self.inner.__arrow_c_stream__(py, requested_schema)
    }

    fn __arrow_c_schema__(&self, py: Python) -> PyResult<PyObject> {
        self.inner.schema_capsule(py) // exact method name to confirm against the pinned
                                       // pyo3-arrow version at implementation time
    }
}
```

### CAP-02 import from a foreign PyCapsule-compliant object
```rust
// Source: derived from pyo3-arrow's FromPyObject impl on PyTable, verified this session
// (accepts any object implementing __arrow_c_stream__/__arrow_c_array__, zero-copy).
#[pyfunction]
fn from_arrow(obj: PyTable) -> Table {
    // `obj: PyTable` in the function signature is enough — PyO3 invokes pyo3-arrow's
    // FromPyObject impl automatically, which detects and consumes the PyCapsule protocol
    // on `obj` (a pyarrow Table, Polars DataFrame, or DuckDB relation) with no data copy.
    Table { inner: obj }
}
```

### D-06a: pointer-identity zero-copy proof (Python side)
```python
# Source: derived from numpy.ndarray.ctypes documentation, verified this session.
import pandas as pd
import flint

def test_from_pandas_zero_copy_pointer_identity():
    df = pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})
    original_ptr = df["a"].array._pa_array.chunk(0).buffers()[1].address  # or equivalent
                                                                            # buffer-address accessor
    table = flint.Table.from_pandas(df)
    exported_ptr = table.column("a").buffer_address(0)  # project-specific accessor
    assert exported_ptr == original_ptr  # same physical memory, not just equal values
```

### D-06b: no-heap-allocation proof (Rust side)
```rust
// Source: derived from allocation-counter crate documentation, verified this session.
#[test]
fn from_pandas_numeric_column_allocates_nothing_for_data_buffer() {
    let numpy_buffer_ptr = /* obtain borrowed buffer pointer for the test fixture */;
    let info = allocation_counter::measure(|| {
        let table = flint_core::from_numpy_buffer(numpy_buffer_ptr /* ... */);
        std::hint::black_box(&table); // prevent optimizer from eliding the conversion
        table
    });
    assert_eq!(info.count_total, 0, "conversion allocated heap memory for the data buffer");
}
```

### DIAG-01/DIAG-03 exception hierarchy (Claude's Discretion — recommendation)
```python
# Recommendation, not a locked decision: a small, catchable hierarchy rather than a bare
# ValueError, so callers can catch "any strict-mode failure" without string-matching messages.
class FlintError(Exception):
    """Base class for all flint-raised errors."""

class ZeroCopyRequiredError(FlintError):
    """Raised in strict zero-copy mode when a column would require a copy."""
    def __init__(self, column: str, dtype: str, reason: str):
        self.column, self.dtype, self.reason = column, dtype, reason
        super().__init__(f"column {column!r} (dtype={dtype}) requires a copy: {reason}")
```

### DIAG-02 `copy_report()` shape (Claude's Discretion — recommendation)
```python
# Recommendation: a list of small, typed records (not a bare dict) so downstream tooling
# (and this project's own tests) get attribute access + easy filtering, mirroring the
# same per-column decision data that powers strict mode (Pitfall 2's shared source of truth).
from dataclasses import dataclass

@dataclass(frozen=True)
class ColumnCopyStatus:
    column: str
    dtype: str
    zero_copy: bool
    reason: str | None  # None when zero_copy is True

# table.copy_report() -> list[ColumnCopyStatus]
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| Raw `_export_to_c`/`_import_from_c` pointer-integer FFI (pre-2024 pyarrow-specific pattern) | Arrow PyCapsule Interface (`__arrow_c_array__`/`__arrow_c_stream__`/`__arrow_c_schema__`), dependency-free | ~2023-2024 (pyarrow ~14, pandas 2.2) | This project should never implement the legacy pattern as primary — already reflected in STACK.md/ARCHITECTURE.md, reaffirmed here at the implementation-pattern level |
| Table-level `zero_copy_only` flags (pyarrow's approach) | Column-level zero-copy eligibility checks | N/A (this project's own design choice, informed by pyarrow's documented failure) | Directly shapes DIAG-01's implementation granularity (Pitfall 2) |
| DuckDB Python consuming Arrow only via a hard pyarrow dependency | DuckDB Python's replacement-scan mechanism recognizing `__arrow_c_stream__`-bearing objects directly (`duckdb.sql("FROM cap")` pattern reported for Polars) | Reported in community/discussion sources during 2024-2025; **exact current-version behavior not independently confirmed this session** | Affects whether the D-05 DuckDB interop test can be pyarrow-free or needs pyarrow as an intermediary — see Open Questions |

**Deprecated/outdated:**
- Raw C-ABI pointer-integer passing for Arrow interop — superseded by PyCapsule Interface (already covered in STACK.md/PITFALLS.md, reaffirmed here).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|----------------|
| A1 | Success Criterion 2 / CONV-01's "non-null numeric/bool" strict-mode-succeeds fixture should use `pandas.ArrowDtype`-backed bool (`"bool[pyarrow]"`), while default numpy-backed bool is treated as an explicit strict-mode-rejected copy-required case. | Summary; Pitfall 1 | If wrong (e.g. the user actually intends numpy-backed bool to be an acceptable "minimal-copy but count-it-as-success" case), the planner would write acceptance tests that either falsely fail on a truly-zero-copy `ArrowDtype` bool fixture, or falsely require a numpy-backed bool copy to be silently accepted — undermining D-03's explicit "no silent success" requirement. This should be confirmed with the user or explicitly locked by the planner before test fixtures are written. |
| A2 | Current DuckDB Python (1.5.x) recognizes objects implementing `__arrow_c_stream__` directly in its replacement-scan mechanism (`duckdb.sql("FROM <capsule-object>")`), without requiring pyarrow as an intermediary. | Standard Stack; State of the Art; Open Questions | If wrong, the D-05 DuckDB interop validation task may need pyarrow installed as an intermediary hop rather than testing a pure PyCapsule-only path — this doesn't block the phase (pyarrow is already a dev dependency) but would change what the interop test is actually proving. Low severity, but should be confirmed with a quick spike (`import duckdb; duckdb.sql("FROM obj")` against a Polars/flint object) at the start of the interop-validation task rather than assumed. |
| A3 | `pyo3-arrow` 0.19.0's exact method names for schema-capsule export on a composed wrapper (used in the Code Examples sketch, e.g. `schema_capsule`) may not match the pinned version's actual API 1:1. | Code Examples | Low risk — this is presented as illustrative composition pattern, not verbatim API; planner/implementer must check `docs.rs/pyo3-arrow/0.19.0` (or whatever version is pinned in `Cargo.toml`) directly before writing the real delegation code. |

## Open Questions (RESOLVED: deferred to execution / discretionary — see plan-checker verification)

1. **RESOLVED — deferred to execution with a guarded fallback (Plan 01-04 Task 2).** Does DuckDB Python (current release, 1.5.x) consume a PyCapsule-protocol object natively via its relation/replacement-scan mechanism, or does it still require pyarrow as an intermediary?
   - What we know: A GitHub discussion thread (`duckdb/duckdb#10716`) explicitly requested PyCapsule support to drop the pyarrow dependency; a more recent search result states "You can use `duckdb.sql("FROM cap")` where `cap = df.__arrow_c_stream__()`... without requiring PyArrow" for Polars specifically, and a separate DuckDB issue (`#17084`, filed against current DuckDB) demonstrates DuckDB's own relations *exporting* via `__arrow_c_stream__()` today.
   - What's unclear: Whether the *import* side (DuckDB recognizing an arbitrary foreign object, specifically this project's `Table`, as a queryable source without pyarrow present) is fully shipped in the currently pinned DuckDB version, versus only partially/recently landed.
   - Recommendation: Spend a short spike at the start of the CAP-01/CAP-02 interop-validation task confirming this empirically against the actual pinned `duckdb` version (`uv run python -c "import duckdb, flint; duckdb.sql('FROM obj').df()"` against a real `Table` instance) before writing the D-05 acceptance test, rather than assuming either way. If native consumption isn't yet reliable, fall back to validating DuckDB round-trip via `duckdb.sql(...).arrow()` / registering through pyarrow as an explicit, documented intermediary step for this phase only.

2. **RESOLVED — discretionary, plans follow this document's sketches as the default (Plan 01-02).** Should the strict-mode/`copy_report()` exception hierarchy and report shape (Code Examples) be locked now or left fully to implementation-time discretion?
   - What we know: CONTEXT.md explicitly marks both as "Claude's Discretion," not locked decisions.
   - What's unclear: Whether the planner should treat the sketches in this document as strong recommendations to encode directly into task acceptance criteria, or leave them as implementation-time judgment calls.
   - Recommendation: Treat the sketches as the default plan (they directly serve D-03's "clear exception naming column/dtype" and D-04's "separate on-demand call" requirements) unless the planner identifies a concrete reason to deviate.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (`rustc`, `cargo`) | Compiling the PyO3 extension at all — every task in this phase | ✗ | — | None — must be installed (`rustup` or distro package) as an explicit Wave 0 / setup task before any Rust code can be written or tested. This blocks execution, not just this phase's success criteria. |
| `maturin` | Building the extension into an importable wheel for Python-side tests | ✗ | — | Installable via `uv add --dev maturin` or `pip install maturin` once a Rust toolchain is present; not a blocking gap on its own, but sequenced after the Rust toolchain install |
| `uv` | Project's mandated Python packaging/dev workflow (PROJECT.md constraint) | ✓ | 0.11.25 | — |
| Python | Running the test suite, pandas/pyarrow/polars/duckdb interop | ✓ | 3.12.3 | — |
| `pip` | Fallback/verification tooling (this research used it for registry checks) | ✓ | present alongside Python 3.12.3 | — |

**Missing dependencies with no fallback:**
- Rust toolchain (`rustc`/`cargo`) — not present in this environment. This is a genuine Wave 0 blocker for the entire phase (and every subsequent Rust phase); the plan must include an explicit toolchain-install task before any `cargo build`/`maturin develop` step.

**Missing dependencies with fallback:**
- `maturin` — not currently installed, but installation is a single, low-risk step once the Rust toolchain exists; include as an early task, not a blocker requiring a design decision.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V2 Authentication | No | Library has no auth surface |
| V3 Session Management | No | No session concept in this domain |
| V4 Access Control | No | No access-control surface in this domain |
| V5 Input Validation | **Yes** | CAP-02 accepts a foreign Python object claiming to implement the Arrow PyCapsule protocol. Must validate the exported `FFI_ArrowSchema`/`FFI_ArrowArray`/`FFI_ArrowArrayStream` structs (non-null pointers, internally consistent buffer lengths/offsets, schema-matches-array-shape) before dereferencing, exactly as PITFALLS.md's Security Mistakes table already flags. Treat every foreign capsule as untrusted input, the same discipline as parsing an untrusted file. |
| V6 Cryptography | No | No cryptographic surface in this domain |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| Malicious/buggy library hands this project's `Table.from_arrow`/CAP-02 import path a capsule with a null or dangling pointer, or a schema/array-length mismatch | Tampering / Denial of Service | Validate capsule contents (null-checks, length/offset bounds, schema-array consistency) before any `unsafe` dereference — `pyo3-arrow`'s existing import path does much of this, but this project's own composition code must not add an unchecked `unsafe` shortcut around it |
| A crafted numpy/pandas buffer with a non-contiguous stride or non-zero offset is misread as a simple contiguous buffer during `from_pandas`, causing an out-of-bounds read | Tampering | Explicitly check contiguity/offset before treating a numpy buffer as directly borrowable (already called out in PITFALLS.md Pitfall 4); fall back to the copy path rather than assuming alignment/contiguity |
| GIL-unsafe refcount decrement on a borrowed Python buffer's owner during a Rust-side `Drop` that runs off the main thread | Tampering (memory corruption) / DoS | Guarantee the release/drop path for any zero-copy-borrowed pandas/numpy buffer reacquires the GIL (`Python::with_gil`) before touching Python refcounts — directly inherited from ARCHITECTURE.md's Flow 2 "Ownership hazard" callout, restated here as a security-relevant memory-safety property, not just a correctness one |

## Sources

### Primary (HIGH confidence)
- `pip index versions {pandas,pyarrow,polars,duckdb,numpy,pytest,hypothesis}` — direct PyPI registry queries, run this session, used to falsify the package-legitimacy tool's "too-new" false positives
- `.planning/research/STACK.md`, `ARCHITECTURE.md`, `PITFALLS.md`, `SUMMARY.md` — project-level research, already HIGH/MEDIUM-confidence per their own metadata, treated as ground truth and not re-derived
- `gsd-tools query package-legitimacy check` — tool output, cross-checked against primary registry data above

### Secondary (MEDIUM confidence, cross-corroborated this session)
- [docs.rs/pyo3-arrow](https://docs.rs/pyo3-arrow/latest/pyo3_arrow/), [PyArray struct docs](https://docs.rs/pyo3-arrow/latest/pyo3_arrow/struct.PyArray.html) — struct-to-PyCapsule-dunder mapping, cross-checked against two independent WebSearch passes
- [arro3 Table API docs](https://kylebarron.dev/arro3/latest/api/core/table/) — fetched directly, confirms no `from_pandas`/`to_pandas`/`copy_report` equivalent exists in the closest prior-art project
- [apache/arrow#39194 — `zero_copy_only=True` never succeeds](https://github.com/apache/arrow/issues/39194), [apache/arrow#38644](https://github.com/apache/arrow/issues/38644) — primary-source GitHub issues, already cited in PITFALLS.md, re-confirmed this session
- [Polars Arrow producer/consumer docs](https://docs.pola.rs/user-guide/misc/arrow/), [polars.from_arrow](https://docs.pola.rs/api/python/stable/reference/api/polars.from_arrow.html) — Polars' PyCapsule Interface support since v1.3
- [duckdb/duckdb#10716](https://github.com/duckdb/duckdb/discussions/10716), [duckdb/duckdb#17084](https://github.com/duckdb/duckdb/issues/17084) — DuckDB PyCapsule feature-request thread and a documented repeated-call bug on DuckDB's own export path; import-side (consuming foreign capsules) current-version status not fully confirmed — see Open Questions
- [allocation-counter (fornwall) crates.io/docs.rs](https://docs.rs/allocation-counter) — allocation-counting API and the optimizer-elision caveat
- [numpy.ndarray.ctypes docs](https://numpy.org/doc/stable/reference/generated/numpy.ndarray.ctypes.html) — pointer-identity mechanism and reference-lifetime caution

### Tertiary (LOW confidence)
- General WebSearch summaries without a fetched primary source for DuckDB's *current* native PyCapsule import behavior — flagged explicitly in Open Questions/Assumptions Log (A2), not presented as settled fact

## Metadata

**Confidence breakdown:**
- Standard stack (this phase's additions): HIGH — all seven new Python packages independently verified via `pip index versions` this session; Rust crates already HIGH-verified in STACK.md
- Architecture / pyo3-arrow API mapping: MEDIUM — cross-corroborated across docs.rs and multiple WebSearch passes, but not fetched as raw source code line-by-line
- Pitfalls (bool trap, strict-mode granularity, allocation-counter caveat): HIGH for the underlying mechanics (numpy/Arrow bool layout difference is a well-established, unambiguous fact; `apache/arrow#39194` is a primary-source bug report); the *recommendation* for how to resolve it (A1) is this document's own synthesis, tagged as an assumption for user/planner confirmation
- DuckDB PyCapsule-native-consumption status: LOW — conflicting/time-ambiguous signals found this session, explicitly flagged as an Open Question rather than asserted

**Research date:** 2026-07-13
**Valid until:** ~14 days for the pyo3-arrow/arro3 API specifics (fast-moving, pre-1.0 crates); ~7 days for the DuckDB PyCapsule-consumption Open Question specifically (recommend re-verifying immediately before the interop-validation task, not relying on this snapshot); ~30 days for the pandas/numpy bool-layout mechanics (stable, unlikely to change)

---
*Phase: 1-core-zero-copy-round-trip-interop*
*Research completed: 2026-07-13*
