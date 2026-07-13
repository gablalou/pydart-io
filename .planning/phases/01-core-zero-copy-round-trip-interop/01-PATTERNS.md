# Phase 1: Core Zero-Copy Round-Trip & Interop - Pattern Map

**Mapped:** 2026-07-13
**Files analyzed:** 12 (all new — greenfield project)
**Analogs found:** 0 / 12 (no existing codebase to search — see "No Analog Found" below)

## Important Caveat: Greenfield Project

This is a brand-new project with no prior code, no existing directory structure, and nothing
committed to the repo yet (`ls -la` confirms only `.git`, `.claude`, `.planning`). Per
CONTEXT.md's "Existing Code Insights": *"Greenfield project — no existing code, no codebase
maps, nothing to reuse yet."*

Consequently, **no in-codebase analog search was possible or performed** beyond confirming the
repo is empty. Every file below is genuinely new. In place of codebase analogs, this document
substitutes the concrete patterns already worked out in RESEARCH.md (Code Examples,
Architecture Patterns) and named external prior art (`arro3`, `pyo3-arrow`). All citations below
point to RESEARCH.md section/line ranges or named external library docs — none are fabricated
codebase line numbers.

**Planner note — file list assumes a decision not yet locked:** The file list and layout below
follow RESEARCH.md's "Recommended Project Structure" (lines 127-152), which assumes the
**two-crate workspace** (`flint-core` + `flint-python`) layout. Per CONTEXT.md, this crate-layout
choice is explicitly **Claude's Discretion**, not a locked decision — research recommends it but
the planner should confirm/own this choice before finalizing plan file paths. If the planner
instead chooses a single crate with a `python` feature, collapse the `flint-core`/`flint-python`
distinction below into one crate's module tree; the role/data-flow classification and pattern
assignments are unaffected either way.

## File Classification

| New File | Role | Data Flow | Closest Analog | Match Quality |
|----------|------|-----------|-----------------|----------------|
| `Cargo.toml` (workspace root) | config | — | none | no analog (greenfield) |
| `crates/flint-core/src/table.rs` | model | CRUD (in-memory representation) | none | no analog (greenfield) |
| `crates/flint-python/src/lib.rs` | config/module-entry | request-response | none | no analog (greenfield) |
| `crates/flint-python/src/table.rs` | binding/controller | request-response + streaming (delegates PyCapsule export) | none in-repo; pattern from `pyo3_arrow::PyTable` (external) | role-match (external library, not codebase) |
| `crates/flint-python/src/pandas.rs` | binding/service (transform) | transform (per-column copy-vs-borrow decision) | none — confirmed original design territory (RESEARCH.md line 49: arro3 has no pandas interop equivalent) | no analog |
| `crates/flint-python/src/diagnostics.rs` | binding/service | request-response (on-demand diagnostics call) | none — confirmed original design territory (RESEARCH.md line 49: arro3 has no `copy_report`-equivalent) | no analog |
| `python/flint/__init__.py` | module-entry | request-response | none | no analog (greenfield) |
| `pyproject.toml` | config | — | none | no analog (greenfield) |
| `tests/rust/zero_copy_alloc.rs` | test | batch (allocation measurement) | none — pattern from `allocation-counter` crate docs (external) | role-match (external library) |
| `tests/python/test_round_trip.py` | test | CRUD round-trip | none | no analog (greenfield) |
| `tests/python/test_zero_copy_pointer.py` | test | request-response (pointer-identity assertion) | none — pattern from `numpy.ndarray.ctypes` docs (external) | role-match (external library) |
| `tests/python/test_strict_mode.py` | test | request-response (error-path) | none | no analog (greenfield) |
| `tests/python/test_copy_report.py` | test | request-response | none | no analog (greenfield) |
| `tests/python/test_interop.py` | test | event-driven/streaming (external consumer round-trip) | none | no analog (greenfield) |

## Pattern Assignments

### `crates/flint-python/src/table.rs` (binding/controller, request-response + streaming)

**Source:** RESEARCH.md "Pattern 1: Compose `pyo3_arrow::PyTable`" (lines 154-157) and Code
Example (lines 269-294).

**Core pattern — compose, don't reimplement PyCapsule marshalling:**
```rust
use pyo3::prelude::*;
use pyo3_arrow::PyTable;

#[pyclass(name = "Table")]
struct Table {
    inner: PyTable,
}

#[pymethods]
impl Table {
    fn __arrow_c_stream__(&self, py: Python, requested_schema: Option<PyObject>) -> PyResult<PyObject> {
        self.inner.__arrow_c_stream__(py, requested_schema)
    }

    fn __arrow_c_schema__(&self, py: Python) -> PyResult<PyObject> {
        self.inner.schema_capsule(py) // confirm exact method name against pinned pyo3-arrow 0.19.0
    }
}
```

**CAP-02 import pattern** (RESEARCH.md lines 296-307, Pattern 2 lines 159-162):
```rust
#[pyfunction]
fn from_arrow(obj: PyTable) -> Table {
    Table { inner: obj }
}
```
Note: must validate the foreign capsule's contents (non-null pointers, schema/array consistency)
before trusting it — see Security Domain in RESEARCH.md (lines 417-435). `pyo3-arrow` handles
mechanical marshalling safely but this project's composition code is still responsible for not
blindly trusting caller-supplied schema/pointer consistency.

**Anti-pattern to avoid** (RESEARCH.md line 194, Pitfall 3): never call a foreign object's
`__arrow_c_stream__()` more than once — DuckDB relations error on a second call. Consume once,
cache the result.

---

### `crates/flint-python/src/pandas.rs` (binding/service, transform)

**Source:** RESEARCH.md "Pattern 3: Per-column dtype-backend detection" (lines 164-189).

**Core pattern — per-column decision logic (the genuinely new code in this phase):**
```rust
enum ColumnPlan {
    ZeroCopyBorrow,
    RequiresCopy { reason: String },
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

**Critical constraint (Pitfall 1, RESEARCH.md lines 209-219):** bool is NOT a uniform case.
numpy-backed bool (1 byte/element) requires a bit-packing copy to become Arrow bool (1
bit/element); only `pandas.ArrowDtype`-backed bool (`"bool[pyarrow]"`) is genuinely zero-copy.
This decision directly shapes both `from_pandas`/`to_pandas` and what strict mode must reject —
do not special-case this away.

**Granularity constraint (Pitfall 2, RESEARCH.md lines 223-233):** this decision MUST be made
per-column, not once for the whole DataFrame — pyarrow's table-level `zero_copy_only` is
documented as essentially non-functional (`apache/arrow#39194`) precisely because it isn't
per-column.

**Security validation (RESEARCH.md lines 429-435):** must check contiguity/offset explicitly
before treating a numpy buffer as directly borrowable; a non-contiguous/offset buffer misread as
contiguous causes an out-of-bounds read.

---

### `crates/flint-python/src/diagnostics.rs` (binding/service, request-response)

**Source:** RESEARCH.md Code Examples, lines 339-368 (exception hierarchy + `copy_report()` shape
— both "Claude's Discretion" recommendations per CONTEXT.md, not locked decisions).

**Exception hierarchy pattern:**
```python
class FlintError(Exception):
    """Base class for all flint-raised errors."""

class ZeroCopyRequiredError(FlintError):
    """Raised in strict zero-copy mode when a column would require a copy."""
    def __init__(self, column: str, dtype: str, reason: str):
        self.column, self.dtype, self.reason = column, dtype, reason
        super().__init__(f"column {column!r} (dtype={dtype}) requires a copy: {reason}")
```

**`copy_report()` shape pattern:**
```python
from dataclasses import dataclass

@dataclass(frozen=True)
class ColumnCopyStatus:
    column: str
    dtype: str
    zero_copy: bool
    reason: str | None  # None when zero_copy is True

# table.copy_report() -> list[ColumnCopyStatus]
```

**Shared source of truth (Pitfall 2, RESEARCH.md line 229):** both strict mode (DIAG-01) and
`copy_report()` (DIAG-02) MUST call the same `plan_column`-style per-column decision function
from `pandas.rs` — do not implement two separate decision paths that could silently disagree.

---

### `tests/rust/zero_copy_alloc.rs` (test, batch/allocation-measurement)

**Source:** RESEARCH.md Code Example D-06b, lines 324-336; Pitfall 4, lines 251-261.

```rust
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

**Warning (Pitfall 4):** `allocation-counter` can produce false negatives if the optimizer proves
the measured closure's result is unused. Always route the converted value through
`std::hint::black_box` or an actual return, and sanity-check the test by deliberately introducing
a `.clone()`/`.to_vec()` to confirm the test would actually catch a non-zero-copy regression.

---

### `tests/python/test_zero_copy_pointer.py` (test, request-response/pointer-identity)

**Source:** RESEARCH.md Code Example D-06a, lines 309-322.

```python
import pandas as pd
import flint

def test_from_pandas_zero_copy_pointer_identity():
    df = pd.DataFrame({"a": pd.array([1, 2, 3], dtype="int64[pyarrow]")})
    original_ptr = df["a"].array._pa_array.chunk(0).buffers()[1].address
    table = flint.Table.from_pandas(df)
    exported_ptr = table.column("a").buffer_address(0)
    assert exported_ptr == original_ptr
```

**Complementary, not redundant** (RESEARCH.md Summary, line 51): this pointer-identity test and
the Rust-side allocation-counter test both must pass — neither alone proves zero-copy.

---

## Shared Patterns

### Error handling: single `From<FlintError> for PyErr` boundary
**Source:** RESEARCH.md "Don't Hand-Roll" table, line 203 (row: "Rust panics/errors crossing into
Python").
**Apply to:** All files under `crates/flint-python/src/` (table.rs, pandas.rs, diagnostics.rs,
lib.rs).
Use `thiserror` for the Rust error type, centralize the Python-exception conversion in one
`impl From<FlintError> for PyErr` at the FFI boundary, rather than scattering `PyErr::new` calls
through conversion code. This is what makes D-03's "clear exception naming column/dtype"
achievable consistently.

### PyCapsule marshalling: delegate to `pyo3-arrow`, never hand-roll
**Source:** RESEARCH.md "Don't Hand-Roll" table, line 200.
**Apply to:** `crates/flint-python/src/table.rs` (export path, CAP-01) and any CAP-02 import code.
Never construct `FFI_ArrowArray`/`FFI_ArrowSchema` structs or PyCapsule destructors by hand —
compose `pyo3_arrow::PyTable`/`PySchema`/`PyArray` instead. Hand-rolling reintroduces the
double-free/leak bug class documented in PITFALLS.md Pitfall 1.

### Per-column decision as single source of truth
**Source:** RESEARCH.md Pitfall 2 (lines 223-233), Pattern 3 (lines 164-189).
**Apply to:** `pandas.rs` (produces the decision) and `diagnostics.rs` (`copy_report()` and
strict-mode both consume it).
Both DIAG-01 (strict mode) and DIAG-02 (`copy_report()`) must read from the same per-column
`plan_column`-equivalent function — never implement the eligibility check twice.

### GIL discipline on buffer release
**Source:** RESEARCH.md Security Domain, line 435.
**Apply to:** Any Rust `Drop` implementation touching a borrowed pandas/numpy buffer's owner
(likely in `pandas.rs` or `table.rs`).
Any refcount decrement on a borrowed Python buffer's owner must reacquire the GIL
(`Python::with_gil`) if running off the main thread — a memory-safety property, not just a
correctness one.

## No Analog Found

All 12 files listed above have no existing in-codebase analog. Reason: this is a confirmed
greenfield project — `ls -la` on the repo root shows only `.git`, `.claude`, `.planning`; no
`src/`, `crates/`, or `python/` directories exist yet. CONTEXT.md's "Existing Code Insights"
section explicitly states: "Greenfield project — no existing code, no codebase maps, nothing to
reuse yet. This phase establishes the foundational patterns... that all later phases build on."

Where a codebase analog would normally go, this document instead cites:
1. **RESEARCH.md Code Examples / Architecture Patterns** (concrete sketches, cited above by line
   range) — the primary substitute source for the planner.
2. **External prior art named in RESEARCH.md**: `arro3` (kylebarron) as the closest sibling
   project — but RESEARCH.md explicitly confirms (line 49) that `arro3`'s own `Table` has *no*
   pandas interop and *no* `copy_report`-equivalent, meaning `pandas.rs` and `diagnostics.rs` are
   genuinely original design territory with no prior art to copy, even externally.
3. **`pyo3-arrow` library docs** (`docs.rs/pyo3-arrow`) for the PyCapsule composition pattern used
   in `table.rs` — a library dependency to compose against, not a codebase analog.

## Metadata

**Analog search scope:** Entire repository root (confirmed empty of source code via `ls -la` and
`find`); no `Glob`/`Grep` search of source patterns was performed beyond this confirmation, since
CONTEXT.md already established the repo is greenfield and no further search would change that
finding.
**Files scanned:** 0 source files (none exist)
**Pattern extraction date:** 2026-07-13
