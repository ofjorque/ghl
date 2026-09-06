use std::collections::HashMap;
use std::fmt;
use ghl_diagnostics::Diagnostic;
use ghl_syntax::ast::{Expr, BinaryOp};
use ghl_types::ContrastScheme;
use crate::env::RuntimeEnv;

pub type NativeFunction = fn(Vec<Value>) -> Result<Value, Diagnostic>;

/// First-class runtime values in GHL.
#[derive(Debug, Clone)]
pub enum Value {
    I64(i64),
    F64(f64),
    Bool(bool),
    String(String),
    Unit,
    /// Missing value with optional semantic reason:
    /// - None => generic NA
    /// - Some("SensorDropout") => NA:SensorDropout
    NA(Option<String>),
    Vector(Vec<Value>),
    Matrix {
        rows: usize,
        cols: usize,
        data: Vec<f64>,
    },
    DataFrame {
        columns: Vec<String>,
        data: HashMap<String, Vec<Value>>,
    },
    ColRef(String),
    ColPredicate {
        col: String,
        op: BinaryOp,
        rhs: Box<Value>,
    },
    Factor {
        levels: Vec<String>,
        indices: Vec<usize>,
        ordered: bool,
        contrast: ContrastScheme,
    },
    Formula {
        response: String,
        terms: Vec<String>,
    },
    Closure {
        params: Vec<String>,
        body: Expr,
        env: RuntimeEnv,
    },
    NativeFn(NativeFunction),
}

impl Value {
    pub fn is_na(&self) -> bool {
        matches!(self, Value::NA(_))
    }

    pub fn na_reason(&self) -> Option<&str> {
        match self {
            Value::NA(Some(r)) => Some(r.as_str()),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::F64(x) => Some(*x),
            Value::I64(n) => Some(*n as f64),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::I64(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::I64(_) => "i64",
            Value::F64(_) => "f64",
            Value::Bool(_) => "bool",
            Value::String(_) => "str",
            Value::Unit => "()",
            Value::NA(_) => "NA",
            Value::Vector(_) => "Vector",
            Value::Matrix { .. } => "Matrix",
            Value::DataFrame { .. } => "DataFrame",
            Value::ColRef(_) => "ColRef",
            Value::ColPredicate { .. } => "ColPredicate",
            Value::Factor { .. } => "Factor",
            Value::Formula { .. } => "Formula",
            Value::Closure { .. } => "Function",
            Value::NativeFn(_) => "NativeFunction",
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::F64(a), Value::F64(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::NA(a), Value::NA(b)) => a == b,
            (Value::Vector(a), Value::Vector(b)) => a == b,
            (
                Value::Matrix { rows: r1, cols: c1, data: d1 },
                Value::Matrix { rows: r2, cols: c2, data: d2 },
            ) => r1 == r2 && c1 == c2 && d1 == d2,
            (
                Value::DataFrame { columns: c1, data: d1 },
                Value::DataFrame { columns: c2, data: d2 },
            ) => c1 == c2 && d1 == d2,
            (Value::ColRef(c1), Value::ColRef(c2)) => c1 == c2,
            (
                Value::ColPredicate { col: c1, op: o1, rhs: r1 },
                Value::ColPredicate { col: c2, op: o2, rhs: r2 },
            ) => c1 == c2 && o1 == o2 && r1 == r2,
            (
                Value::Factor { levels: l1, indices: i1, ordered: o1, contrast: k1 },
                Value::Factor { levels: l2, indices: i2, ordered: o2, contrast: k2 },
            ) => l1 == l2 && i1 == i2 && o1 == o2 && k1 == k2,
            (
                Value::Formula { response: r1, terms: t1 },
                Value::Formula { response: r2, terms: t2 },
            ) => r1 == r2 && t1 == t2,
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::I64(n) => write!(f, "{}", n),
            Value::F64(x) => write!(f, "{}", x),
            Value::Bool(b) => write!(f, "{}", b),
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::Unit => write!(f, "()"),
            Value::NA(None) => write!(f, "NA"),
            Value::NA(Some(r)) => write!(f, "NA:{}", r),
            Value::Vector(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Matrix { rows, cols, data } => {
                writeln!(f, "Matrix[{}x{}]:", rows, cols)?;
                for r in 0..*rows {
                    write!(f, "  [ ")?;
                    for c in 0..*cols {
                        if c > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{:>6.2}", data[r * cols + c])?;
                    }
                    writeln!(f, " ]")?;
                }
                Ok(())
            }
            Value::DataFrame { columns, data } => {
                let num_rows = columns
                    .first()
                    .and_then(|c| data.get(c))
                    .map(|v| v.len())
                    .unwrap_or(0);
                writeln!(f, "DataFrame ({} rows x {} cols):", num_rows, columns.len())?;
                for col in columns {
                    write!(f, "{:>12} ", col)?;
                }
                writeln!(f)?;
                for _ in columns {
                    write!(f, "{:>12} ", "------------")?;
                }
                writeln!(f)?;
                for r in 0..num_rows {
                    for col in columns {
                        if let Some(col_data) = data.get(col) {
                            if let Some(val) = col_data.get(r) {
                                write!(f, "{:>12} ", format!("{}", val))?;
                            } else {
                                write!(f, "{:>12} ", "NA")?;
                            }
                        }
                    }
                    writeln!(f)?;
                }
                Ok(())
            }
            Value::ColRef(c) => write!(f, "col(\"{}\")", c),
            Value::ColPredicate { col, op, rhs } => write!(f, "col(\"{}\") {:?} {}", col, op, rhs),
            Value::Factor {
                levels,
                indices,
                ordered,
                contrast,
            } => {
                let kind = if *ordered { "OrderedFactor" } else { "Factor" };
                write!(
                    f,
                    "{}[n={}, levels={:?}, contrast={:?}]",
                    kind,
                    indices.len(),
                    levels,
                    contrast
                )
            }
            Value::Formula { response, terms } => {
                write!(f, "{} ~ {}", response, terms.join(" + "))
            }
            Value::Closure { params, .. } => {
                write!(f, "fn({}) -> <closure>", params.join(", "))
            }
            Value::NativeFn(_) => write!(f, "<native_fn>"),
        }
    }
}
