"""Flint: a Rust-backed, zero-copy pandas <-> Arrow interop layer.

`Table` (and, in later plans, `from_arrow`/`FlintError`/`ZeroCopyRequiredError`/
`ColumnCopyStatus`) are implemented in the compiled `_flint` Rust extension and re-exported here.
"""

from ._flint import Table

__all__ = ["Table"]
