//! Single source-of-truth Parquet read/write logic (PARQ-01, basic PARQ-02/D-28).
//!
//! `write_parquet`/`read_parquet` are the ONLY functions that touch the `parquet` crate directly
//! -- `crates/flint-python/src/table.rs`'s `to_parquet`/`from_parquet` `#[pymethods]` parse the
//! Python-facing `str`/`pathlib.Path` argument into a `PathBuf` and then delegate here, never
//! re-deriving any `ArrowWriter`/`ParquetRecordBatchReaderBuilder` call themselves. Plans 02-04
//! extend this module (compression/row-group config, predicate pushdown, multi-file) without
//! re-deriving IO logic elsewhere -- mirrors `pandas_plan.rs`'s single-decision-point discipline.
//!
//! This module has no `pyo3`/`pyo3-arrow` dependency (see `flint-core`'s crate-level doc comment)
//! so the write/read logic itself is unit-testable without a Python interpreter attached. Errors
//! are surfaced as `parquet::errors::ParquetError` (this crate cannot depend on `flint-python`'s
//! `FlintError`, which itself depends on `pyo3` -- that would be a circular dependency); the
//! PyO3 boundary in `flint-python/src/table.rs` maps `ParquetError` onto `FlintError::Other`.
//!
//! Every parquet-crate `Result` is `?`-propagated -- this module NEVER calls `.unwrap()`/
//! `.expect()` on a parse result for file bytes that did not come from this project's own writer
//! in the same process (T-03-01): a malformed/corrupt Parquet file surfaces as a `ParquetError`,
//! never a panic that aborts the interpreter.

use std::fs::File;
use std::path::Path;

use arrow::compute::concat_batches;
use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::errors::ParquetError;

/// Write a single `RecordBatch` to a Parquet file at `path`.
///
/// Uses `std::fs::File::create` (D-22: truncates/overwrites an existing file silently, no
/// `overwrite=` guard) and the crate-default `WriterProperties` (`None`), which is snappy
/// compression (D-28). Does NOT call `ArrowWriterOptions::with_skip_arrow_metadata()` -- the
/// default embeds the full Arrow schema (`ARROW:schema` metadata key), which is what preserves
/// `DataType::Dictionary`/tz-aware-timestamp fidelity on read (verified by the Wave-0 A6 gate,
/// `tests/rust/parquet_dictionary_tz_roundtrip.rs`).
///
/// A 0-row `batch` still writes a valid, schema-only Parquet file (the empty-table decision) --
/// `ArrowWriter` has no special-cased zero-row behavior to work around here, writing a 0-row
/// batch produces a valid file carrying just the schema/footer.
///
/// NOTE: this plan uses the crate-default `WriterProperties` (`None` -> snappy); Plan 02 replaces
/// the `None` with a built `WriterProperties` carrying the codec + row_group_size, keeping this
/// signature Plan-02-extensible.
pub fn write_parquet(batch: &RecordBatch, path: &Path) -> Result<(), ParquetError> {
    let file = File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, batch.schema(), None)?;
    writer.write(batch)?;
    writer.close()?;
    Ok(())
}

/// Read a Parquet file at `path` back into a single `RecordBatch`.
///
/// Does NOT call `ArrowReaderOptions::with_schema(..)` -- the reader consults the embedded
/// `ARROW:schema` hint by default (same fidelity mechanism as `write_parquet`'s doc comment).
///
/// A Parquet file can be split across multiple row groups, each yielding its own `RecordBatch`
/// from the reader; these are concatenated into a single `RecordBatch` via
/// `arrow::compute::concat_batches` (an honest, `?`-propagated concat, not a first-batch-only
/// truncation -- the same discipline that fixed CR-01 in `flint-python/src/pandas.rs`). A file
/// with zero row groups/batches (the empty-table case) returns a 0-row `RecordBatch` built from
/// the reader's own resolved schema, rather than erroring.
pub fn read_parquet(path: &Path) -> Result<RecordBatch, ParquetError> {
    let file = File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let schema = builder.schema().clone();
    let reader = builder.build()?;

    let mut batches: Vec<RecordBatch> = Vec::new();
    for batch in reader {
        batches.push(batch?);
    }

    if batches.is_empty() {
        return Ok(RecordBatch::new_empty(schema));
    }
    if batches.len() == 1 {
        return Ok(batches.into_iter().next().expect("checked len == 1 above"));
    }
    concat_batches(&schema, &batches).map_err(ParquetError::from)
}
