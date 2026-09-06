//! Type system definitions, Factor types, and NA reasoning for GHL.
//!
//! Implements RFC 02 (Type System & NA semantics) and RFC 09 (Factors & Categorical Data).

pub mod types;
pub mod env;
pub mod checker;

pub use types::Type;
pub use env::{TypeEnv, SymbolInfo};
pub use checker::TypeChecker;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NAReason {
    NoResponse,
    NotObservable,
    NotApplicable,
    SensorDropout,
    BelowDetectionLimit,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum GhlValue<T> {
    Value(T),
    NA(Option<NAReason>),
}

impl<T> GhlValue<T> {
    pub fn is_na(&self) -> bool {
        match self {
            Self::NA(_) => true,
            _ => false,
        }
    }

    pub fn na_reason(&self) -> Option<&NAReason> {
        match self {
            Self::NA(Some(reason)) => Some(reason),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContrastScheme {
    Treatment,
    Sum,
    Helmert,
    Polynomial,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Factor {
    pub levels: Vec<String>,
    pub indices: Vec<usize>,
    pub contrast: ContrastScheme,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderedFactor {
    pub factor: Factor,
}

/// Helper function to check a program AST.
pub fn check(
    program: &ghl_syntax::ast::Program,
    filename: &str,
) -> Result<TypeEnv, Vec<ghl_diagnostics::Diagnostic>> {
    let mut checker = TypeChecker::new(filename.to_string());
    checker.check_program(program).map(|_| checker.env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghl_syntax::parser::parse;

    #[test]
    fn test_na_with_optional_reason() {
        let plain_na: GhlValue<f64> = GhlValue::NA(None);
        let reasoned_na: GhlValue<f64> = GhlValue::NA(Some(NAReason::NoResponse));
        let observed: GhlValue<f64> = GhlValue::Value(42.0);

        assert!(plain_na.is_na());
        assert_eq!(plain_na.na_reason(), None);

        assert!(reasoned_na.is_na());
        assert_eq!(reasoned_na.na_reason(), Some(&NAReason::NoResponse));

        assert!(!observed.is_na());
        assert_eq!(observed.na_reason(), None);
    }

    #[test]
    fn test_typecheck_valid_script() {
        let code = r#"
            let df = dataframe {
                age: [25, 30, 45],
                income: [50000.0, 75000.5, 60000.0]
            };

            let summary = df |> filter(col("age") > 18);
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh");
        assert!(res.is_ok(), "Type checking should pass for valid pipeline");
    }

    #[test]
    fn test_typecheck_reject_string_plus_float() {
        // Anti-R: No silent string coercion in arithmetic
        let code = r#"let bad = "hello" + 42.0;"#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh");
        assert!(res.is_err(), "Cannot add string and float");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0102"));
    }

    #[test]
    fn test_typecheck_reject_heterogeneous_vector() {
        // Anti-R: [1, "foo"] must not silently become ["1", "foo"]
        let code = r#"let bad = [1, "foo"];"#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh");
        assert!(res.is_err(), "Heterogeneous vector must be rejected");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0103"));
    }

    #[test]
    fn test_typecheck_vector_na_preserves_int() {
        // Anti-Python: NA does not degrade Vector[I64] to Vector[F64]
        let code = r#"let v: Vector[i64] = [1, 2, NA, NA:SensorDropout, 5];"#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh");
        assert!(res.is_ok(), "Vector with NA should keep integer type");
    }

    #[test]
    fn test_typecheck_matrix_solve() {
        let code = r#"
            let A = mat [ 1.0, 2.0 ; 3.0, 4.0 ];
            let b = [5.0, 6.0];
            let x = A \ b;
        "#;
        let program = parse(code).expect("syntax ok");
        let env = check(&program, "test.gh").expect("type check ok");
        let x_sym = env.lookup("x").expect("x must exist");
        assert_eq!(x_sym.ty, Type::Vector(Box::new(Type::F64)));
    }

    #[test]
    fn test_typecheck_if_else_branches_mismatch() {
        let code = r#"
            let res = if 10 > 5 {
                "text"
            } else {
                42
            };
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh");
        assert!(res.is_err(), "Incompatible if/else branches must error");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0102"));
    }
}


