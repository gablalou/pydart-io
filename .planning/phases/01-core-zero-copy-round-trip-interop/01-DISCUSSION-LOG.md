# Phase 1: Core Zero-Copy Round-Trip & Interop - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-13
**Phase:** 1-core-zero-copy-round-trip-interop
**Areas discussed:** Public API naming, Diagnostics API shape, PyCapsule interop scope, Zero-copy proof strategy

---

## Public API Naming

| Option | Description | Selected |
|--------|-------------|----------|
| Table (Recommended) | Matches pyarrow/Polars/DuckDB convention — minimizes friction for the exact audience you're targeting | ✓ |
| Distinct name | e.g. FlintTable — signals this is not a pyarrow drop-in | |
| Let me explain | User has a specific name/convention in mind | |

**User's choice:** Table (Recommended)
**Notes:** —

| Option | Description | Selected |
|--------|-------------|----------|
| from_pandas / to_pandas (Recommended) | Exact match to pyarrow's naming — migration can often just change the import | ✓ |
| Different verbs | e.g. from_dataframe/to_dataframe — more generic | |

**User's choice:** from_pandas / to_pandas (Recommended)
**Notes:** —

---

## Diagnostics API Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Raise an exception (Recommended) | Clear error naming the offending column/dtype — avoids pyarrow's zero_copy_only credibility gap | ✓ |
| Return a result + warning | Succeeds but flags that a copy occurred | |

**User's choice:** Raise an exception (Recommended)
**Notes:** —

| Option | Description | Selected |
|--------|-------------|----------|
| Separate diagnostics call (Recommended) | e.g. table.copy_report() — doesn't clutter the normal conversion return value | ✓ |
| Bundled into conversion result | Conversion call returns (Table, diagnostics) or attaches diagnostics as an attribute | |

**User's choice:** Separate diagnostics call (Recommended)
**Notes:** —

---

## PyCapsule Interop Scope

| Option | Description | Selected |
|--------|-------------|----------|
| pyarrow + Polars + DuckDB (Recommended) | Validates the actual ecosystem-interop claim from day one | ✓ |
| pyarrow only for now | Narrower Phase 1 scope, defer Polars/DuckDB validation | |

**User's choice:** pyarrow + Polars + DuckDB (Recommended)
**Notes:** —

---

## Zero-Copy Proof Strategy

| Option | Description | Selected |
|--------|-------------|----------|
| Both pointer identity + allocation counting (Recommended) | Strongest proof — catches both aliasing bugs and hidden copies | ✓ |
| Pointer identity only | Simpler: just assert buffer address unchanged | |

**User's choice:** Both pointer identity + allocation counting (Recommended)
**Notes:** —

---

## Claude's Discretion

- Crate layout (single Rust crate with a `python` feature vs. two-crate workspace mirroring `arro3`) — technical architecture decision, research recommends two-crate shape.
- Exact exception type/hierarchy for strict-mode failures, and exact return shape of `copy_report()` (dict vs. dataclass vs. named object).

## Deferred Ideas

None — discussion stayed within Phase 1 scope.
