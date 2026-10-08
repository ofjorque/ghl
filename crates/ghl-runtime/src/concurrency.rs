//! Parallel Iterators backed by Rayon work-stealing (RFC 05 §2).
//!
//! Provides thread-safe, work-stealing parallel data pipelines for GHL scripts.
//! Supports `(start..end).par_iter()` and `Vector.par_iter()` with chained
//! `map`, `filter`, `reduce`, `sum`, `count`, `min`, `max`, and `collect`.

use crate::eval::Interpreter;
use crate::value::Value;
use crate::vector_data::VectorData;
use ghl_diagnostics::Diagnostic;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Constructs a first-class `ParallelIterator` struct from a vector of items.
pub fn make_parallel_iterator(items: Vec<Value>) -> Value {
    let mut fields = BTreeMap::new();
    fields.insert(
        "__items".into(),
        Value::Vector(VectorData::from_values(items)),
    );
    Value::Struct {
        name: "ParallelIterator".into(),
        fields: Arc::new(fields),
    }
}

/// Extracts items from a `ParallelIterator`, `Range`, or `Vector`.
pub fn extract_parallel_items(val: &Value) -> Result<Vec<Value>, Diagnostic> {
    match val {
        Value::Struct { name, fields } if name == "ParallelIterator" => {
            if let Some(Value::Vector(vd)) = fields.get("__items") {
                Ok(vd.iter().cloned().collect())
            } else {
                Err(Diagnostic::compute_error(
                    "C0201",
                    "Corrupted ParallelIterator struct: missing __items",
                ))
            }
        }
        &Value::Range {
            start,
            end,
            inclusive,
        } => {
            let items: Vec<Value> = if inclusive {
                (start..=end).map(Value::I64).collect()
            } else {
                (start..end).map(Value::I64).collect()
            };
            Ok(items)
        }
        Value::Vector(vd) => Ok(vd.iter().cloned().collect()),
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!(
                "Expected ParallelIterator, Range, or Vector, found `{}`",
                other.type_name()
            ),
        )),
    }
}

/// Native entry point for `par_iter(collection)` or `collection.par_iter()`.
pub fn native_par_iter(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0101", "par_iter requires 1 argument (Range or Vector)")
    })?;
    let items = extract_parallel_items(target)?;
    Ok(make_parallel_iterator(items))
}

/// `ParallelIterator::map(self, transform_closure)`
pub fn native_par_map(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0101",
            "ParallelIterator::map requires 2 arguments: (self, transform_closure)",
        ));
    }
    let items = extract_parallel_items(&args[0])?;
    let callable = args[1].clone();
    let env_template = interp.env.clone();

    let transformed: Result<Vec<Value>, Diagnostic> = items
        .into_par_iter()
        .map(|item| {
            let mut local_interp = Interpreter::with_env(env_template.clone());
            local_interp.call_value(callable.clone(), vec![item])
        })
        .collect();

    Ok(make_parallel_iterator(transformed?))
}

/// `ParallelIterator::filter(self, predicate_closure)`
pub fn native_par_filter(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0101",
            "ParallelIterator::filter requires 2 arguments: (self, predicate_closure)",
        ));
    }
    let items = extract_parallel_items(&args[0])?;
    let callable = args[1].clone();
    let env_template = interp.env.clone();

    let matches: Result<Vec<bool>, Diagnostic> = items
        .par_iter()
        .map(|item| {
            let mut local_interp = Interpreter::with_env(env_template.clone());
            let res = local_interp.call_value(callable.clone(), vec![item.clone()])?;
            match res {
                Value::Bool(b) => Ok(b),
                other => Err(Diagnostic::compute_error(
                    "C0102",
                    format!(
                        "Predicate in parallel filter must return bool, found `{}`",
                        other.type_name()
                    ),
                )),
            }
        })
        .collect();

    let flags = matches?;
    let mut filtered = Vec::new();
    for (item, keep) in items.into_iter().zip(flags) {
        if keep {
            filtered.push(item);
        }
    }

    Ok(make_parallel_iterator(filtered))
}

/// `ParallelIterator::collect(self)` -> `Value::Vector`
pub fn native_par_collect(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0101",
            "ParallelIterator::collect requires 1 argument (self)",
        )
    })?;
    let items = extract_parallel_items(target)?;
    Ok(Value::Vector(VectorData::from_values(items)))
}

/// `ParallelIterator::reduce(self, binary_op)`
pub fn native_par_reduce(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0101",
            "ParallelIterator::reduce requires 2 arguments: (self, binary_op)",
        ));
    }
    let items = extract_parallel_items(&args[0])?;
    if items.is_empty() {
        return Ok(Value::Unit);
    }
    let callable = args[1].clone();
    let env_template = interp.env.clone();

    let outcome: Option<Result<Value, Diagnostic>> =
        items.into_par_iter().map(Ok).reduce_with(|a_res, b_res| {
            let a = a_res?;
            let b = b_res?;
            let mut local_interp = Interpreter::with_env(env_template.clone());
            local_interp.call_value(callable.clone(), vec![a, b])
        });

    match outcome {
        Some(res) => res,
        None => Ok(Value::Unit),
    }
}

/// `ParallelIterator::sum(self)`
pub fn native_par_sum(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0101", "ParallelIterator::sum requires 1 argument (self)")
    })?;
    let items = extract_parallel_items(target)?;

    let all_ints = items.iter().all(|it| matches!(it, Value::I64(_)));
    if all_ints {
        let s: i64 = items
            .par_iter()
            .map(|it| match it {
                &Value::I64(n) => n,
                _ => 0,
            })
            .sum();
        Ok(Value::I64(s))
    } else {
        let s: f64 = items.par_iter().map(|it| it.as_f64().unwrap_or(0.0)).sum();
        Ok(Value::F64(s))
    }
}

/// `ParallelIterator::count(self)`
pub fn native_par_count(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0101",
            "ParallelIterator::count requires 1 argument (self)",
        )
    })?;
    let items = extract_parallel_items(target)?;
    Ok(Value::I64(items.len() as i64))
}
