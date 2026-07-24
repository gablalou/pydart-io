//! Single source-of-truth Parquet read/write logic (PARQ-01..PARQ-05, D-28).
//!
//! `write_parquet`/`read_parquet` are the ONLY functions that touch the `parquet` crate directly
//! -- `crates/flint-python/src/table.rs`'s `to_parquet`/`from_parquet` `#[pymethods]` parse the
//! Python-facing `str`/`pathlib.Path`/filter-tuple/column-list arguments and then delegate here,
//! never re-deriving any `ArrowWriter`/`ParquetRecordBatchReaderBuilder` call themselves. Plan 04
//! extends this module (multi-file, dtype fidelity) without re-deriving IO logic elsewhere --
//! mirrors `pandas_plan.rs`'s single-decision-point discipline.
//!
//! This module has no `pyo3`/`pyo3-arrow` dependency (see `flint-core`'s crate-level doc comment)
//! so the write/read logic itself is unit-testable without a Python interpreter attached. Errors
//! are surfaced as `parquet::errors::ParquetError` (this crate cannot depend on `flint-python`'s
//! `FlintError`, which itself depends on `pyo3` -- that would be a circular dependency); the
//! PyO3 boundary in `flint-python/src/table.rs` maps `ParquetError` onto `FlintError::Other`.
//! `FlintError::UnsupportedFilterOperator` (D-25 rejection) is raised entirely at that boundary,
//! BEFORE any `FilterExpr` is built -- this module never sees an unrecognized operator string.
//!
//! Every parquet-crate `Result` is `?`-propagated -- this module NEVER calls `.unwrap()`/
//! `.expect()` on a parse result for file bytes that did not come from this project's own writer
//! in the same process (T-03-01): a malformed/corrupt Parquet file surfaces as a `ParquetError`,
//! never a panic that aborts the interpreter.
//!
//! ## Predicate pushdown (PARQ-04/PARQ-05, D-26/D-27)
//!
//! `read_parquet`'s `filters: &[FilterExpr]` is parsed ONCE at the PyO3 boundary and consumed by
//! BOTH `surviving_row_groups` (the row-group-level skip decision, built on
//! `parquet::arrow::arrow_reader::statistics::StatisticsConverter` + `parquet_filter::
//! could_match_range`) AND `build_row_filter` (the row-level `RowFilter`/`ArrowPredicateFn`
//! builder) -- never re-parsed or re-derived between the two (RESEARCH.md Anti-Patterns; the
//! project's single-decision-point convention). Row-group skipping is an optimization layered
//! UNDER the exact row-level filter, never a replacement for it: even if pruning under- or
//! over-*keeps* row groups (conservative on any doubt), `RowFilter` still guarantees the returned
//! `Table` contains ONLY matching rows (D-26).
//!
//! Column projection (`columns: Option<&[String]>`, D-27) is independent of `filters` -- a
//! `ProjectionMask` restricted to the requested columns controls what is physically decoded, and
//! is entirely separate from the per-filter `ProjectionMask`s `build_row_filter` uses to evaluate
//! predicates (a filter column need not appear in the output projection). Because
//! `ProjectionMask::columns` preserves the row groups' SCHEMA order (not necessarily the caller's
//! requested order), `read_parquet` reorders the decoded batch via `RecordBatch::project` to match
//! the caller's exact `columns` order after decoding.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, AsArray, BooleanArray, Float64Array, Int64Array, StringArray};
use arrow::compute::kernels::cmp;
use arrow::compute::{cast, concat_batches};
use arrow::datatypes::{
    DataType, Field, Float32Type, Float64Type, Int16Type, Int32Type, Int64Type, Int8Type, Schema,
    UInt16Type, UInt32Type, UInt8Type,
};
use arrow::error::ArrowError;
use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::statistics::StatisticsConverter;
use parquet::arrow::arrow_reader::{ArrowPredicate, ArrowPredicateFn, ParquetRecordBatchReaderBuilder, RowFilter};
use parquet::arrow::{ArrowWriter, ProjectionMask};
use parquet::basic::{Compression, GzipLevel, ZstdLevel};
use parquet::errors::ParquetError;
use parquet::file::metadata::ParquetMetaData;
use parquet::file::properties::WriterProperties;
use parquet::schema::types::SchemaDescriptor;

use crate::parquet_filter::{could_match_range, FilterExpr, Op, ScalarValue};

/// Build a `WriterProperties` for `write_parquet` from a D-29 codec string and a D-30 row-count
/// row-group size.
///
/// The codec match is EXHAUSTIVE over the four D-29-locked strings ("snappy", "zstd", "gzip",
/// "uncompressed") with an explicit error arm for anything else -- never a silent
/// `.unwrap_or(Compression::SNAPPY)` default (T-03-03 / the project's no-silent-best-effort
/// pattern). `codec` is the SOLE fallible input here: callers must validate `row_group_size != 0`
/// themselves before calling (see `table.rs::to_parquet`), since
/// `set_max_row_group_row_count(Some(0))` panics rather than returning a `Result` -- keeping this
/// function's only failure mode the codec match lets the PyO3 boundary map every `Err` here
/// directly onto `FlintError::UnsupportedCodec` without misattributing a different failure.
///
/// Uses `set_max_row_group_row_count` (row-count semantics, D-30) rather than the deprecated
/// `set_max_row_group_size` (deprecated since parquet 58.0.0) or the byte-based
/// `set_max_row_group_bytes` -- confirmed directly against the pinned `parquet = "59.1.0"` source
/// (`file/properties.rs`), per Pitfall 4 / Assumption A1.
pub fn build_writer_properties(
    codec: &str,
    row_group_size: usize,
) -> Result<WriterProperties, ParquetError> {
    let compression = match codec {
        "snappy" => Compression::SNAPPY,
        "zstd" => Compression::ZSTD(ZstdLevel::default()), // parameterized variant (Pitfall 3)
        "gzip" => Compression::GZIP(GzipLevel::default()), // parameterized variant (Pitfall 3)
        "uncompressed" => Compression::UNCOMPRESSED,
        other => {
            return Err(ParquetError::General(format!(
                "unsupported compression codec {other:?}"
            )))
        }
    };
    Ok(WriterProperties::builder()
        .set_compression(compression)
        .set_max_row_group_row_count(Some(row_group_size))
        .build())
}

/// Write a single `RecordBatch` to a Parquet file at `path` using the given `WriterProperties`.
///
/// Uses `std::fs::File::create` (D-22: truncates/overwrites an existing file silently, no
/// `overwrite=` guard). `properties` is built by `build_writer_properties` (or, in tests, any
/// other valid `WriterProperties`) -- the embedded `ARROW:schema` metadata that preserves
/// `DataType::Dictionary`/tz-aware-timestamp fidelity on read (verified by the Wave-0 A6 gate,
/// `tests/rust/parquet_dictionary_tz_roundtrip.rs`) is `ArrowWriter`'s always-on default and is
/// unaffected by any `WriterProperties` compression/row-group setting.
///
/// A 0-row `batch` still writes a valid, schema-only Parquet file (the empty-table decision) --
/// `ArrowWriter` has no special-cased zero-row behavior to work around here, writing a 0-row
/// batch produces a valid file carrying just the schema/footer.
pub fn write_parquet(
    batch: &RecordBatch,
    path: &Path,
    properties: WriterProperties,
) -> Result<(), ParquetError> {
    let file = File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(properties))?;
    writer.write(batch)?;
    writer.close()?;
    Ok(())
}

/// Read a Parquet file at `path` back into a single `RecordBatch`, applying an optional column
/// projection (D-27) and an AND-combined list of filters (D-23/D-24, PARQ-04/PARQ-05).
///
/// Does NOT call `ArrowReaderOptions::with_schema(..)` -- the reader consults the embedded
/// `ARROW:schema` hint by default (same fidelity mechanism as `write_parquet`'s doc comment).
///
/// `columns: None` / `filters: &[]` reproduces Plan 01's unfiltered/unprojected behavior exactly.
/// See the module doc comment for the full pushdown/projection contract.
///
/// A Parquet file can be split across multiple (surviving) row groups, each yielding its own
/// `RecordBatch` from the reader; these are concatenated into a single `RecordBatch` via
/// `arrow::compute::concat_batches` (an honest, `?`-propagated concat, not a first-batch-only
/// truncation -- the same discipline that fixed CR-01 in `flint-python/src/pandas.rs`). A file
/// with zero surviving row groups/batches returns a 0-row `RecordBatch` built from the reader's
/// own resolved (already-projected) schema, rather than erroring.
pub fn read_parquet(
    path: &Path,
    columns: Option<&[String]>,
    filters: &[FilterExpr],
) -> Result<RecordBatch, ParquetError> {
    let file = File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let arrow_schema: std::sync::Arc<Schema> = builder.schema().clone();
    let parquet_schema: SchemaDescriptor = builder.parquet_schema().clone();
    let metadata: std::sync::Arc<ParquetMetaData> = std::sync::Arc::clone(builder.metadata());

    let surviving = surviving_row_groups(&metadata, &arrow_schema, &parquet_schema, filters)?;
    let mut reader_builder = builder.with_row_groups(surviving);

    if !filters.is_empty() {
        let row_filter = build_row_filter(&parquet_schema, filters);
        reader_builder = reader_builder.with_row_filter(row_filter);
    }

    // The output projection (D-27) is built independently of any per-filter evaluation mask --
    // `build_row_filter` constructs its OWN single-column `ProjectionMask` per predicate above.
    let output_mask = match columns {
        Some(cols) => ProjectionMask::columns(&parquet_schema, cols.iter().map(String::as_str)),
        None => ProjectionMask::all(),
    };
    reader_builder = reader_builder.with_projection(output_mask);

    let reader = reader_builder.build()?;
    let projected_schema = arrow::record_batch::RecordBatchReader::schema(&reader);

    let mut batches: Vec<RecordBatch> = Vec::new();
    for batch in reader {
        batches.push(batch?);
    }

    let mut batch = if batches.is_empty() {
        RecordBatch::new_empty(projected_schema)
    } else if batches.len() == 1 {
        batches.into_iter().next().expect("checked len == 1 above")
    } else {
        concat_batches(&projected_schema, &batches).map_err(ParquetError::from)?
    };

    // `ProjectionMask::columns` preserves declared-schema order, not necessarily the caller's
    // requested order (D-27's "in the requested order" contract) -- reorder via
    // `RecordBatch::project` to match `columns` exactly.
    if let Some(cols) = columns {
        let indices: Result<Vec<usize>, ArrowError> = cols
            .iter()
            .map(|name| batch.schema().index_of(name))
            .collect();
        batch = batch
            .project(&indices.map_err(ParquetError::from)?)
            .map_err(ParquetError::from)?;
    }

    Ok(batch)
}

/// Errors specific to reading MULTIPLE Parquet files as one `Table` (D-21).
///
/// Kept separate from the plain `ParquetError` `read_parquet` uses, so the PyO3 boundary
/// (`flint-python/src/table.rs`) can construct the precise, named
/// `FlintError::ParquetSchemaMismatch`/`FlintError::ParquetReadError` variant directly from
/// structured fields, without string-sniffing an underlying `ParquetError`'s message -- the same
/// "structured info crosses the flint-core/flint-python boundary" discipline as
/// `build_writer_properties`'s codec-only-fallible design (Plan 02).
#[derive(Debug)]
pub enum MultiParquetReadError {
    /// Two files' Arrow schemas disagree. Carries the first file, the first mismatched file,
    /// and the first differing column name (D-21: never a silent union/merge across files).
    SchemaMismatch {
        first_file: String,
        other_file: String,
        column: String,
    },
    /// A specific file failed to open/parse. Carries the offending path so the caller can name
    /// it directly (a missing file, non-Parquet file, or corrupt file never surfaces as a
    /// generic, unattributed error).
    Read { path: String, source: ParquetError },
}

/// Read one or more Parquet files (D-21: a single file, a caller-provided list, or a
/// directory's `.parquet` files, already resolved to `paths` by the PyO3 boundary) into ONE
/// `RecordBatch`, applying the SAME optional column projection (D-27) and filter list
/// (D-23/D-24, PARQ-04/PARQ-05) to every file via `read_parquet`.
///
/// `paths` MUST be non-empty (the PyO3 boundary rejects an empty list/directory before calling
/// here, per D-21's empty-edge decision). A single-element `paths` behaves identically to
/// calling `read_parquet` directly (D-21 empty/single edge).
///
/// BEFORE decoding each file's data, every file's RAW (unprojected, unfiltered) Arrow schema is
/// compared against the first file's for STRICT equality (D-21 Open Question 1: strict-match
/// required, never a silent best-effort union/merge) -- a mismatch returns
/// `MultiParquetReadError::SchemaMismatch` naming the first file, the first mismatched file, and
/// the first differing column, before any row is read. Matching files are read (with the
/// caller's `columns`/`filters` applied per file, reusing `read_parquet`'s single-file logic
/// unchanged) and concatenated via `arrow::compute::concat_batches` in the caller's file-list
/// order (D-21 ordering edge: explicit list order as given; the PyO3 boundary sorts a
/// directory's discovered files lexicographically before calling here).
pub fn read_parquet_multi(
    paths: &[PathBuf],
    columns: Option<&[String]>,
    filters: &[FilterExpr],
) -> Result<RecordBatch, MultiParquetReadError> {
    let first_path = &paths[0];
    let first_schema = read_raw_schema(first_path).map_err(|source| MultiParquetReadError::Read {
        path: first_path.display().to_string(),
        source,
    })?;

    let mut batches: Vec<RecordBatch> = Vec::with_capacity(paths.len());
    for path in paths {
        let schema = read_raw_schema(path).map_err(|source| MultiParquetReadError::Read {
            path: path.display().to_string(),
            source,
        })?;
        if let Some(column) = first_schema_mismatch(&first_schema, &schema) {
            return Err(MultiParquetReadError::SchemaMismatch {
                first_file: first_path.display().to_string(),
                other_file: path.display().to_string(),
                column,
            });
        }

        let batch = read_parquet(path, columns, filters).map_err(|source| MultiParquetReadError::Read {
            path: path.display().to_string(),
            source,
        })?;
        batches.push(batch);
    }

    if batches.len() == 1 {
        return Ok(batches.into_iter().next().expect("checked len == 1 above"));
    }

    let schema = batches[0].schema();
    concat_batches(&schema, &batches).map_err(|err| MultiParquetReadError::Read {
        path: first_path.display().to_string(),
        source: ParquetError::from(err),
    })
}

/// Read a single Parquet file's own (unprojected, unfiltered) Arrow schema -- used by
/// `read_parquet_multi`'s strict cross-file schema-equality check, independent of any
/// projection/filter the caller may apply to the actual data read.
fn read_raw_schema(path: &Path) -> Result<Arc<Schema>, ParquetError> {
    let file = File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    Ok(builder.schema().clone())
}

/// Return the name of the first column at which two schemas disagree (by name, `DataType`, and
/// nullability), or `None` if every field matches positionally. Deliberately narrower than
/// `Field`'s full `PartialEq` (which also compares dictionary-ordering/metadata) -- this project
/// cares about D-21's "the same logical column" contract, not incidental metadata differences a
/// writer might attach.
fn first_schema_mismatch(a: &Schema, b: &Schema) -> Option<String> {
    let a_fields = a.fields();
    let b_fields = b.fields();
    let max_len = a_fields.len().max(b_fields.len());
    for i in 0..max_len {
        match (a_fields.get(i), b_fields.get(i)) {
            (Some(fa), Some(fb)) => {
                if !fields_match(fa, fb) {
                    return Some(fa.name().clone());
                }
            }
            (Some(fa), None) => return Some(fa.name().clone()),
            (None, Some(fb)) => return Some(fb.name().clone()),
            (None, None) => unreachable!("i < max_len guarantees at least one side has a field"),
        }
    }
    None
}

/// Compare two `Field`s for D-21's "same logical column" purposes: name, `DataType`, and
/// nullability -- NOT `Field`'s full `PartialEq` (which also compares dictionary-ordering and
/// arbitrary metadata that can legitimately differ between two files carrying the same logical
/// column).
fn fields_match(a: &Field, b: &Field) -> bool {
    a.name() == b.name() && a.data_type() == b.data_type() && a.is_nullable() == b.is_nullable()
}

/// Decide, for each row group, whether it MIGHT contain a row matching every filter in `filters`
/// (D-24: AND-combined). Consumes the SAME parsed `filters` list `build_row_filter` consumes --
/// never re-parsed (single-source-of-truth, see module doc comment).
///
/// A row group is skipped only when at least one filter's `could_match_range` provably rules it
/// out; row groups already excluded by an earlier filter are not re-checked against later ones
/// (short-circuit is a pure optimization, not a correctness dependency -- final AND semantics are
/// identical either way).
pub fn surviving_row_groups(
    metadata: &ParquetMetaData,
    arrow_schema: &Schema,
    parquet_schema: &SchemaDescriptor,
    filters: &[FilterExpr],
) -> Result<Vec<usize>, ParquetError> {
    let num_row_groups = metadata.num_row_groups();
    let mut keep = vec![true; num_row_groups];

    for filter in filters {
        let converter = StatisticsConverter::try_new(&filter.column, arrow_schema, parquet_schema)?;
        let mins = converter.row_group_mins(metadata.row_groups().iter())?;
        let maxes = converter.row_group_maxes(metadata.row_groups().iter())?;

        for (row_group_idx, keep_slot) in keep.iter_mut().enumerate() {
            if !*keep_slot {
                continue; // already excluded by an earlier AND-combined filter
            }
            let min = scalar_from_array(mins.as_ref(), row_group_idx);
            let max = scalar_from_array(maxes.as_ref(), row_group_idx);
            if !could_match_range(filter.op, &filter.value, min.as_ref(), max.as_ref()) {
                *keep_slot = false;
            }
        }
    }

    Ok((0..num_row_groups).filter(|idx| keep[*idx]).collect())
}

/// Build a `RowFilter` performing exact row-level filtering (D-26's row-level half) from the SAME
/// parsed `filters` list `surviving_row_groups` consumes. Multiple filters become multiple
/// `ArrowPredicate`s in one `RowFilter`, which the reader applies conjunctively (D-24: AND-only).
fn build_row_filter(parquet_schema: &SchemaDescriptor, filters: &[FilterExpr]) -> RowFilter {
    let predicates: Vec<Box<dyn ArrowPredicate>> = filters
        .iter()
        .map(|filter| {
            // Each predicate's OWN ProjectionMask decodes only the single column it evaluates --
            // separate from (and possibly excluding) the caller's output projection (D-27).
            let projection = ProjectionMask::columns(parquet_schema, std::iter::once(filter.column.as_str()));
            let op = filter.op;
            let value = filter.value.clone();
            let predicate = ArrowPredicateFn::new(projection, move |batch: RecordBatch| {
                evaluate_predicate(&batch, op, &value)
            });
            Box::new(predicate) as Box<dyn ArrowPredicate>
        })
        .collect();
    RowFilter::new(predicates)
}

/// Evaluate one `FilterExpr`'s comparison against a single-column `RecordBatch` (the column
/// selected by `ArrowPredicateFn`'s own `ProjectionMask`, always at index 0).
///
/// Builds a length-1 literal array in the value's own native representation, then CASTS it to the
/// column's actual `DataType` before comparing -- this makes the comparison correct regardless of
/// the exact numeric width (e.g. a Python `int` filter literal against an `Int32`/`Float64`
/// column) without this project having to hand-write a comparison per Arrow physical type.
fn evaluate_predicate(batch: &RecordBatch, op: Op, value: &ScalarValue) -> Result<BooleanArray, ArrowError> {
    let column = batch.column(0);
    let literal = scalar_value_to_array(value);
    let casted = cast(&literal, column.data_type())?;
    let scalar = arrow::array::Scalar::new(casted);

    match op {
        Op::Eq => cmp::eq(column, &scalar),
        Op::Ne => cmp::neq(column, &scalar),
        Op::Lt => cmp::lt(column, &scalar),
        Op::Le => cmp::lt_eq(column, &scalar),
        Op::Gt => cmp::gt(column, &scalar),
        Op::Ge => cmp::gt_eq(column, &scalar),
    }
}

/// Build a length-1 `ArrayRef` in a `ScalarValue`'s own native Arrow representation (Int64/
/// Float64/Boolean/Utf8) -- the caller then `cast`s this to the target column's actual `DataType`.
fn scalar_value_to_array(value: &ScalarValue) -> ArrayRef {
    match value {
        ScalarValue::Int64(v) => Arc::new(Int64Array::from(vec![*v])),
        ScalarValue::Float64(v) => Arc::new(Float64Array::from(vec![*v])),
        ScalarValue::Bool(v) => Arc::new(BooleanArray::from(vec![*v])),
        ScalarValue::Utf8(v) => Arc::new(StringArray::from(vec![v.clone()])),
    }
}

/// Extract a `ScalarValue` from a `StatisticsConverter`-produced min/max `ArrayRef` at
/// `row_group_idx`. Returns `None` (conservatively "no stat available") when the value is null
/// (the row group's stats are absent -- `StatisticsConverter`'s own documented null-means-unknown
/// convention) OR when the physical type is one this project does not trust for pruning: `Utf8`/
/// `LargeUtf8` min/max statistics in Parquet can be TRUNCATED by the writer, which could cause
/// `could_match_range` to over-prune (silently drop matching rows, the D-26-forbidden direction)
/// -- string columns are still filtered exactly via `evaluate_predicate`'s `RowFilter`, they
/// simply never benefit from the row-group-skip optimization.
fn scalar_from_array(array: &dyn Array, idx: usize) -> Option<ScalarValue> {
    if array.is_null(idx) {
        return None;
    }
    match array.data_type() {
        DataType::Int8 => Some(ScalarValue::Int64(array.as_primitive::<Int8Type>().value(idx) as i64)),
        DataType::Int16 => Some(ScalarValue::Int64(array.as_primitive::<Int16Type>().value(idx) as i64)),
        DataType::Int32 => Some(ScalarValue::Int64(array.as_primitive::<Int32Type>().value(idx) as i64)),
        DataType::Int64 => Some(ScalarValue::Int64(array.as_primitive::<Int64Type>().value(idx))),
        DataType::UInt8 => Some(ScalarValue::Int64(array.as_primitive::<UInt8Type>().value(idx) as i64)),
        DataType::UInt16 => Some(ScalarValue::Int64(array.as_primitive::<UInt16Type>().value(idx) as i64)),
        DataType::UInt32 => Some(ScalarValue::Int64(array.as_primitive::<UInt32Type>().value(idx) as i64)),
        DataType::Float32 => Some(ScalarValue::Float64(array.as_primitive::<Float32Type>().value(idx) as f64)),
        DataType::Float64 => Some(ScalarValue::Float64(array.as_primitive::<Float64Type>().value(idx))),
        DataType::Boolean => Some(ScalarValue::Bool(array.as_boolean().value(idx))),
        // Utf8/LargeUtf8 and any other physical type: conservatively "no stat available" (see
        // doc comment above for the Utf8/LargeUtf8-specific truncation rationale).
        _ => None,
    }
}
