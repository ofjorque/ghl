use std::fmt;
use ghl_syntax::ast::TypeAnnotation;
use crate::ContrastScheme;

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
    /// 2D dense matrix with linear algebra operators.
    Matrix(Box<Type>),
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
    /// First-class function type.
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    /// Standalone missing value literal (NA or NA:Reason).
    /// Unifies with any type without contaminating integers into floats.
    NA,
    /// Dynamic or generic placeholder for unconstrained inference.
    Any,
}

impl Type {
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
        matches!(self, Type::Matrix(_))
    }

    /// Returns the element type if this is a Vector or Matrix.
    pub fn element_type(&self) -> Option<&Type> {
        match self {
            Type::Vector(inner) | Type::Matrix(inner) => Some(inner),
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
            (Type::Matrix(a), Type::Matrix(b)) => {
                let inner = a.unify(b)?;
                Some(Type::Matrix(Box::new(inner)))
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
                "()" | "unit" | "void" => Type::Unit,
                _ => Type::Any,
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
                    Type::Matrix(Box::new(inner))
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
            Type::Matrix(inner) => write!(f, "Matrix[{}]", inner),
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
            Type::NA => write!(f, "NA"),
            Type::Any => write!(f, "any"),
        }
    }
}
