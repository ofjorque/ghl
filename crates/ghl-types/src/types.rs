use std::fmt;
use ghl_syntax::ast::TypeAnnotation;
use crate::ContrastScheme;

/// Matrix dimension in static type checking (RFC 02 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dim {
    Known(usize),
    Dynamic,
}

impl Dim {
    pub fn is_compatible_with(&self, other: &Dim) -> bool {
        match (self, other) {
            (Dim::Known(a), Dim::Known(b)) => a == b,
            _ => true,
        }
    }
}

impl fmt::Display for Dim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Dim::Known(n) => write!(f, "{}", n),
            Dim::Dynamic => write!(f, "Dynamic"),
        }
    }
}

impl From<usize> for Dim {
    fn from(n: usize) -> Self {
        Dim::Known(n)
    }
}

/// Formal types in GHL's static type system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// 64-bit signed integer (real scalar, unlike R's length-1 vectors).
    I64,
    /// 64-bit IEEE-754 floating point number.
    F64,
    /// Boolean (true / false).
    Bool,
    /// UTF-8 String.
    String,
    /// Unit type ().
    Unit,
    /// Homogeneous columnar or memory vector.
    Vector(Box<Type>),
    /// 2D dense matrix with linear algebra operators and static dimensions (RFC 02 §3).
    Matrix {
        elem: Box<Type>,
        rows: Dim,
        cols: Dim,
    },
    /// Columnar dataset with named typed columns.
    DataFrame(Vec<(String, Type)>),
    /// Categorical factor with explicit levels and contrast coding.
    Factor {
        levels: Vec<String>,
        ordered: bool,
        contrast: ContrastScheme,
    },
    /// Statistical modeling formula (response ~ predictors).
    Formula,
    /// SEM/CFA model specification: an ordered list of `Formula` equations
    /// (`~`, `=~`, `~~`) declared inside `sem_spec { ... }`.
    SemSpec,
    /// Fitted statistical model (NEKO framework).
    ModelFit,
    /// Grammar of Graphics statistical plot.
    Plot,
    /// First-class function type.
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    /// Standalone missing value literal (NA or NA:Reason).
    /// Unifies with any type without contaminating integers into floats.
    NA,
    /// Structural Record / Named Tuple: { field1: Type, field2: Type }
    Record(Vec<(String, Type)>),
    /// User-defined struct instance (RFC 02 §4).
    Struct {
        name: String,
        fields: Vec<(String, Type)>,
    },
    /// Custom nominal type reference (e.g. struct/trait name).
    Custom(String),
    /// Dynamic or generic placeholder for unconstrained inference.
    Any,
}

impl Type {
    pub fn matrix(elem: Type, rows: impl Into<Dim>, cols: impl Into<Dim>) -> Self {
        Type::Matrix {
            elem: Box::new(elem),
            rows: rows.into(),
            cols: cols.into(),
        }
    }

    pub fn matrix_dynamic(elem: Type) -> Self {
        Type::Matrix {
            elem: Box::new(elem),
            rows: Dim::Dynamic,
            cols: Dim::Dynamic,
        }
    }

    /// Checks whether this type is numeric (i64 or f64).
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::I64 | Type::F64)
    }

    /// Checks whether this type is a vector.
    pub fn is_vector(&self) -> bool {
        matches!(self, Type::Vector(_))
    }

    /// Checks whether this type is a matrix.
    pub fn is_matrix(&self) -> bool {
        matches!(self, Type::Matrix { .. })
    }

    /// Returns the element type if this is a Vector or Matrix.
    pub fn element_type(&self) -> Option<&Type> {
        match self {
            Type::Vector(inner) | Type::Matrix { elem: inner, .. } => Some(inner),
            _ => None,
        }
    }

    /// Attempts to unify two types under GHL's strict safety rules.
    ///
    /// Anti-Trap Guarantees:
    /// - No silent string coercion (R flaw): `1` and `"foo"` fail to unify.
    /// - No float contamination (Python flaw): `[1, 2, NA]` stays `Vector[i64]`.
    /// - No silent int-to-float promotion without explicit intent.
    pub fn unify(&self, other: &Type) -> Option<Type> {
        if self == other {
            return Some(self.clone());
        }

        match (self, other) {
            // NA unifies with any concrete type, preserving the concrete type!
            (Type::NA, concrete) | (concrete, Type::NA) => Some(concrete.clone()),

            // Any unifies with any type
            (Type::Any, concrete) | (concrete, Type::Any) => Some(concrete.clone()),

            // Vector unification
            (Type::Vector(a), Type::Vector(b)) => {
                let inner = a.unify(b)?;
                Some(Type::Vector(Box::new(inner)))
            }

            // Matrix unification
            (
                Type::Matrix { elem: a, rows: r1, cols: c1 },
                Type::Matrix { elem: b, rows: r2, cols: c2 },
            ) => {
                let inner = a.unify(b)?;
                if !r1.is_compatible_with(r2) || !c1.is_compatible_with(c2) {
                    return None;
                }
                let rows = match (r1, r2) {
                    (Dim::Known(n), _) | (_, Dim::Known(n)) => Dim::Known(*n),
                    _ => Dim::Dynamic,
                };
                let cols = match (c1, c2) {
                    (Dim::Known(n), _) | (_, Dim::Known(n)) => Dim::Known(*n),
                    _ => Dim::Dynamic,
                };
                Some(Type::Matrix {
                    elem: Box::new(inner),
                    rows,
                    cols,
                })
            }

            // Functions
            (
                Type::Function { params: p1, ret: r1 },
                Type::Function { params: p2, ret: r2 },
            ) => {
                if p1.len() != p2.len() {
                    return None;
                }
                let mut unified_params = Vec::new();
                for (a, b) in p1.iter().zip(p2.iter()) {
                    unified_params.push(a.unify(b)?);
                }
                let unified_ret = r1.unify(r2)?;
                Some(Type::Function {
                    params: unified_params,
                    ret: Box::new(unified_ret),
                })
            }
            (Type::Record(f1), Type::Record(f2)) => {
                if f1.len() != f2.len() {
                    return None;
                }
                let mut unified = Vec::new();
                for ((n1, t1), (n2, t2)) in f1.iter().zip(f2.iter()) {
                    if n1 != n2 {
                        return None;
                    }
                    unified.push((n1.clone(), t1.unify(t2)?));
                }
                Some(Type::Record(unified))
            }
            (Type::Struct { name: n1, fields: f1 }, Type::Struct { name: n2, fields: f2 }) => {
                if n1 != n2 || f1.len() != f2.len() {
                    return None;
                }
                let mut unified = Vec::new();
                for ((k1, t1), (k2, t2)) in f1.iter().zip(f2.iter()) {
                    if k1 != k2 {
                        return None;
                    }
                    unified.push((k1.clone(), t1.unify(t2)?));
                }
                Some(Type::Struct {
                    name: n1.clone(),
                    fields: unified,
                })
            }
            (Type::Custom(n1), Type::Custom(n2)) if n1 == n2 => Some(Type::Custom(n1.clone())),
            (s @ Type::Struct { name, .. }, Type::Custom(c)) | (Type::Custom(c), s @ Type::Struct { name, .. }) if name == c => {
                Some(s.clone())
            }

            _ => None,
        }
    }

    /// Convert a syntax AST TypeAnnotation into a semantic Type.
    pub fn from_annotation(ann: &TypeAnnotation) -> Self {
        match ann {
            TypeAnnotation::Simple(name) => match name.as_str() {
                "i64" | "int" => Type::I64,
                "f64" | "float" => Type::F64,
                "bool" => Type::Bool,
                "str" | "string" => Type::String,
                "DataFrame" | "dataframe" => Type::DataFrame(Vec::new()),
                "Factor" | "factor" => Type::Factor {
                    levels: Vec::new(),
                    ordered: false,
                    contrast: ContrastScheme::Treatment,
                },
                "Formula" | "formula" => Type::Formula,
                "SemSpec" | "semspec" => Type::SemSpec,
                "ModelFit" | "modelfit" => Type::ModelFit,
                "Plot" | "plot" => Type::Plot,
                "()" | "unit" | "void" => Type::Unit,
                other => Type::Custom(other.to_string()),
            },
            TypeAnnotation::Generic(name, args) => match name.as_str() {
                "Vector" | "vector" => {
                    let inner = args
                        .first()
                        .map(Self::from_annotation)
                        .unwrap_or(Type::Any);
                    Type::Vector(Box::new(inner))
                }
                "Matrix" | "matrix" => {
                    let inner = args
                        .first()
                        .map(Self::from_annotation)
                        .unwrap_or(Type::F64);
                    let parse_dim = |ann: Option<&TypeAnnotation>| -> Dim {
                        match ann {
                            Some(TypeAnnotation::Simple(s)) => {
                                if let Ok(n) = s.parse::<usize>() {
                                    Dim::Known(n)
                                } else {
                                    Dim::Dynamic
                                }
                            }
                            _ => Dim::Dynamic,
                        }
                    };
                    let rows = parse_dim(args.get(1));
                    let cols = parse_dim(args.get(2));
                    Type::Matrix {
                        elem: Box::new(inner),
                        rows,
                        cols,
                    }
                }
                "Factor" | "factor" => Type::Factor {
                    levels: Vec::new(),
                    ordered: false,
                    contrast: ContrastScheme::Treatment,
                },
                _ => Type::Any,
            },
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::I64 => write!(f, "i64"),
            Type::F64 => write!(f, "f64"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "str"),
            Type::Unit => write!(f, "()"),
            Type::Vector(inner) => write!(f, "Vector[{}]", inner),
            Type::Matrix { elem, rows, cols } => match (rows, cols) {
                (Dim::Dynamic, Dim::Dynamic) => write!(f, "Matrix[{}]", elem),
                (r, c) => write!(f, "Matrix[{}, {}, {}]", elem, r, c),
            },
            Type::DataFrame(cols) => {
                if cols.is_empty() {
                    write!(f, "DataFrame")
                } else {
                    write!(f, "DataFrame {{ ")?;
                    for (i, (name, ty)) in cols.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}: {}", name, ty)?;
                    }
                    write!(f, " }}")
                }
            }
            Type::Factor { ordered, .. } => {
                if *ordered {
                    write!(f, "OrderedFactor")
                } else {
                    write!(f, "Factor")
                }
            }
            Type::Formula => write!(f, "Formula"),
            Type::SemSpec => write!(f, "SemSpec"),
            Type::ModelFit => write!(f, "ModelFit"),
            Type::Plot => write!(f, "Plot"),
            Type::Function { params, ret } => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", ret)
            }
            Type::Record(fields) => {
                write!(f, "{{ ")?;
                for (i, (name, ty)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", name, ty)?;
                }
                write!(f, " }}")
            }
            Type::Struct { name, .. } => write!(f, "{}", name),
            Type::Custom(name) => write!(f, "{}", name),
            Type::NA => write!(f, "NA"),
            Type::Any => write!(f, "any"),
        }
    }
}
