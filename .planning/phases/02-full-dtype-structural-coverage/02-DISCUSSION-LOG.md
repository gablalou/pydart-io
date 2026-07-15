# Phase 2: Full Dtype & Structural Coverage - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-15
**Phase:** 2-full-dtype-structural-coverage
**Areas discussed:** Null handling scope, String/object dtype scope, Multi-chunk handling strategy, Datetime/timezone/timedelta, Categorical fidelity

---

## Null handling scope (CONV-03)

| Option | Description | Selected |
|--------|-------------|----------|
| ArrowDtype-backed only | Only pandas.ArrowDtype nullable columns, narrower scope, zero-copy | ✓ |
| Also pandas nullable extension dtypes | Int64/boolean/Float64 masked arrays using pd.NA | |
| You decide | Let planner pick | |

**User's choice:** ArrowDtype-backed only.
**Notes:** Consistent with Phase 1's DtypeBackend::Arrow path.

| Option | Description | Selected |
|--------|-------------|----------|
| Leave as-is (NaN = value, not null) | Numpy float+NaN keeps going through existing zero-copy path unchanged | ✓ |
| Detect NaN and map to Arrow nulls | Scan for NaN, set null bitmap, breaks zero-copy borrow | |
| You decide | | |

**User's choice:** Leave as-is.
**Notes:** Numpy int dtype can never have NaN (ints have no NaN representation); this only ever concerned float columns.

---

## String/object dtype scope (CONV-04)

| Option | Description | Selected |
|--------|-------------|----------|
| ArrowDtype strings only | Zero-copy, narrower, reject numpy object dtype | |
| Also accept numpy object-dtype strings | Honest copy via RequiresCopy, broader real-world compat | ✓ |
| You decide | | |

**User's choice:** Also accept numpy object-dtype strings.
**Notes:** Per PITFALLS.md Pitfall 3 — object dtype has no Arrow-compatible layout, must be an honestly-reported copy.

| Option | Description | Selected |
|--------|-------------|----------|
| Validate all-string (or None), error otherwise | Introspect column, reject any non-string element with a named error | ✓ |
| Best-effort str() coercion | Convert everything via str(), broader but riskier | |
| You decide | | |

**User's choice:** Validate all-string (or None), error otherwise.
**Notes:** Matches the project's existing explicit-rejection philosophy — no silent best-effort coercion.

---

## Multi-chunk handling strategy (CONV-08 + DIAG-01/DIAG-02 override)

| Option | Description | Selected |
|--------|-------------|----------|
| Honest copy (fix diagnostics only) | Keep concat, make plan_column chunk-count-aware so diagnostics tell the truth | ✓ |
| Genuine zero-copy multi-chunk | Preserve chunk structure via zero-copy alignment — bigger architectural lift | |
| You decide | | |

**User's choice:** Honest copy (fix diagnostics only).
**Notes:** Matches how the Phase 1 override framed the deferred work. Genuine zero-copy multi-chunk would require per-column independent chunk-boundary alignment, which arrow-rs's uniform-row-count RecordBatch model doesn't directly support — judged too large a lift for this phase.

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, strict should now reject chunked input | Correcting the previously-silent bug per DIAG-01's contract | ✓ |
| No — add an opt-in to allow rechunking under strict | New allow_rechunk flag | |

**User's choice:** Yes, strict should now reject chunked input.
**Notes:** Confirmed as an intentional, understood behavior change for existing callers relying on the (buggy) silent-success path.

---

## Datetime/timezone/timedelta (CONV-06/CONV-07)

| Option | Description | Selected |
|--------|-------------|----------|
| Nanosecond-only | Only datetime64[ns]/timedelta64[ns], reject other resolutions | ✓ |
| Support all pandas time units | s/ms/us/ns, broader but more surface area | |
| You decide | | |

**User's choice:** Nanosecond-only.
**Notes:** Matches pandas' overwhelmingly common default and Arrow's matching int64-ns physical layout.

| Option | Description | Selected |
|--------|-------------|----------|
| Preserve tz exactly, reject ambiguous/DST edge cases only if arrow-rs itself errors | Trust pandas/Arrow's own tz handling, no hand-rolled DST logic | ✓ |
| Normalize all tz-aware columns to UTC internally | Extra normalization layer not clearly needed | |

**User's choice:** Preserve tz exactly, reject ambiguous/DST edge cases only if arrow-rs itself errors.
**Notes:** Avoids reinventing a known-hard problem; both pandas and Arrow already store the same int64-ns-since-epoch layout plus a separate tz label.

---

## Categorical fidelity (CONV-05)

| Option | Description | Selected |
|--------|-------------|----------|
| Preserve ordered flag + category order exactly | Full fidelity round-trip | ✓ |
| Preserve category values only, drop ordered/order fidelity | Simpler but changes an inspectable DataFrame property silently | |

**User's choice:** Preserve ordered flag + category order exactly.
**Notes:** Consistent with the project's precision-over-convenience posture established elsewhere in this discussion.

| Option | Description | Selected |
|--------|-------------|----------|
| Preserve exact code width | Round-trip the same int8/16/32/64 code width pandas chose | ✓ |
| Normalize to one fixed width (e.g. always int32) | Simpler implementation, changes .cat.codes.dtype | |
| You decide | | |

**User's choice:** Preserve exact code width.
**Notes:** Reconstructed Categorical's `.cat.codes.dtype` must match the source exactly.

---

## Claude's Discretion

- Exact rejection error message/type for pandas nullable extension dtypes and non-ns-resolution datetime/timedelta dtypes — follow the existing `FlintError::UnsupportedColumn` pattern.
- Internal representation details for preserving categorical code width and order.
- Whether/how `ColumnConversionRecord`'s `reason` field gains new categories to distinguish "structural copy due to multi-chunk" from "dtype-driven copy".

## Deferred Ideas

- Genuine zero-copy multi-chunk preservation — larger architectural undertaking, revisit post-v1 if the honest-copy fallback proves insufficient.
- Pandas nullable extension dtypes (Int64/boolean/Float64 masked arrays) — revisit if user demand emerges post-v1.
- Non-nanosecond datetime/timedelta resolution (datetime64[s]/[ms]/[us]) — revisit if user demand emerges post-v1.
