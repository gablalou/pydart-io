"""Flint: a Rust-backed, zero-copy pandas <-> Arrow interop layer.

`Table`, `FlintError`, and `ZeroCopyRequiredError` are implemented in the compiled `_flint` Rust
extension and re-exported here. `ColumnCopyStatus` (the `Table.copy_report()` record shape, D-04)
is a plain Python dataclass -- constructed from Rust (`crate::diagnostics::build_copy_report`) by
importing this module and calling the class, rather than a `pyo3`-native type.
"""

from dataclasses import dataclass

from ._flint import FlintError, Table, ZeroCopyRequiredError


@dataclass(frozen=True)
class ColumnCopyStatus:
    """One column's zero-copy diagnostics, as returned by `Table.copy_report()` (DIAG-02, D-04).

    `reason` is `None` exactly when `zero_copy` is `True` -- derived from the same `plan_column`
    decision that strict mode (`from_pandas(df, strict=True)`) consumes, so the two features can
    never silently disagree (RESEARCH.md Pitfall 2).
    """

    column: str
    dtype: str
    zero_copy: bool
    reason: str | None


__all__ = ["Table", "FlintError", "ZeroCopyRequiredError", "ColumnCopyStatus"]
