//! Type system definitions, Factor types, and NA reasoning for GHL.
//!
//! Implements RFC 02 (Type System & NA semantics) and RFC 09 (Factors & Categorical Data).

pub mod types;
pub mod env;
pub mod checker;
pub mod modules;

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
///
/// `source` must be the exact text `program` was parsed from - the checker
/// uses it to translate AST byte spans into real line/column numbers for
/// diagnostics.
pub fn check(
    program: &ghl_syntax::ast::Program,
    filename: &str,
    source: &str,
) -> Result<TypeEnv, Vec<ghl_diagnostics::Diagnostic>> {
    let mut checker = TypeChecker::new(filename.to_string(), source);
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
        let res = check(&program, "test.gh", code);
        assert!(res.is_ok(), "Type checking should pass for valid pipeline");
    }

    #[test]
    fn test_typecheck_sem_spec_valid() {
        let code = r#"
            let spec = sem_spec {
                f1 =~ x1 + x2 + x3;
                f2 =~ y1 + y2 + y3;
                f1 ~~ f2;
                f2 ~ f1;
            };
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_ok(), "A sem_spec of formula-only equations should type-check: {:?}", res.err());
    }

    #[test]
    fn test_typecheck_reject_sem_spec_non_formula_equation() {
        let code = r#"
            let spec = sem_spec {
                f1 =~ x1 + x2;
                42;
            };
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_err(), "A non-formula equation inside sem_spec must be rejected");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0615"));
    }

    #[test]
    fn test_typecheck_bare_column_verbs() {
        // Bare column names (no quotes, no `col()`) inside filter/group_by/summarize/arrange
        // must type-check without "undefined variable" diagnostics — the checker's `col_ctx`
        // mirrors the interpreter's ColRef fallback (see ghl_syntax::ast::COLUMN_CONTEXT_VERBS).
        let code = r#"
            let df = dataframe {
                species: ["a", "b", "a"],
                x: [1.0, 2.0, 3.0]
            };

            let filtered = df |> filter(x > 1.0);
            let summary = df |> group_by(species) |> summarize(n = count(), mean_x = mean(x));
            let sorted = df |> arrange(desc(x), species);
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_ok(), "Bare column verbs should type-check cleanly: {:?}", res.err());
    }

    #[test]
    fn test_typecheck_reject_string_plus_float() {
        // Anti-R: No silent string coercion in arithmetic
        let code = r#"let bad = "hello" + 42.0;"#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_err(), "Cannot add string and float");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0102"));
    }

    #[test]
    fn test_typecheck_reject_heterogeneous_vector() {
        // Anti-R: [1, "foo"] must not silently become ["1", "foo"]
        let code = r#"let bad = [1, "foo"];"#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_err(), "Heterogeneous vector must be rejected");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0103"));
    }

    #[test]
    fn test_typecheck_reject_non_mut_assignment() {
        // The first real use of `is_mut` (tracked since Fase 0, never enforced until
        // now) -- reassigning a `let` binding declared without `mut` must be rejected.
        let code = r#"
            let x = 1;
            x = 2;
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_err(), "Assigning to a non-mut binding must be rejected");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0104"));
    }

    #[test]
    fn test_typecheck_reject_assign_to_undeclared() {
        let code = r#"x = 1;"#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_err(), "Assigning to an undeclared variable must be rejected");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0101"));
    }

    #[test]
    fn test_typecheck_allow_mut_assignment() {
        let code = r#"
            let mut x = 1;
            x = 2;
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_ok(), "Assigning to a mut binding of the same type must be allowed");
    }

    #[test]
    fn test_typecheck_reject_while_non_bool_condition() {
        let code = r#"
            while 5 {
                1;
            };
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_err(), "A non-bool while condition must be rejected");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0102"));
    }

    #[test]
    fn test_typecheck_vector_na_preserves_int() {
        // Anti-Python: NA does not degrade Vector[I64] to Vector[F64]
        let code = r#"let v: Vector[i64] = [1, 2, NA, NA:SensorDropout, 5];"#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
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
        let env = check(&program, "test.gh", code).expect("type check ok");
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
        let res = check(&program, "test.gh", code);
        assert!(res.is_err(), "Incompatible if/else branches must error");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0102"));
    }

    #[test]
    fn test_typecheck_use_and_qualified_paths() {
        let code = r#"
            use std::dataframe::read_parquet;
            use std::stats::distributions::{random_normal, normal_pdf};
            use std::linalg::*;
            use std::linalg::transpose as t;

            let y = std::math::sqrt(16.0);
            let const_pi = std::math::pi;
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_ok(), "Type checking should pass for use and qualified paths: {:?}", res.err());
        let env = res.unwrap();
        assert!(env.lookup("read_parquet").is_some());
        assert!(env.lookup("random_normal").is_some());
        assert!(env.lookup("normal_pdf").is_some());
        assert!(env.lookup("dot").is_some());
        assert!(env.lookup("t").is_some());
    }

    #[test]
    fn test_typecheck_reject_invalid_module() {
        let code = r#"use std::fake_mod::something;"#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_err());
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0105"));
    }

    #[test]
    fn test_typecheck_reject_invalid_module_item() {
        let code = r#"use std::math::nonexistent_fn;"#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_err());
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0106"));
    }

    #[test]
    fn test_typecheck_record_literal_and_field_access() {
        let code = r#"
            let rec = { sample_id: "SMP-001", replicates: 4, p_value: 0.0042 };
            let s_id = rec.sample_id;
            let rep = rec.replicates;
        "#;
        let program = parse(code).expect("syntax ok");
        let env = check(&program, "test.gh", code).expect("type check ok");

        let s_id_sym = env.lookup("s_id").expect("s_id must exist");
        assert_eq!(s_id_sym.ty, Type::String);

        let rep_sym = env.lookup("rep").expect("rep must exist");
        assert_eq!(rep_sym.ty, Type::I64);
    }

    #[test]
    fn test_typecheck_bracket_indexing_and_slicing() {
        let code = r#"
            let v = [1.0, 2.0, 3.0, 4.0];
            let elem = v[0];
            let sub_v = v[0..2];
            let m = mat [ 1.0, 2.0 ; 3.0, 4.0 ];
            let m_elem = m[0, 1];
            let m_sub = m[0..1, :];
        "#;
        let program = parse(code).expect("syntax ok");
        let env = check(&program, "test.gh", code).expect("type check ok");

        assert_eq!(env.lookup("elem").unwrap().ty, Type::F64);
        assert_eq!(env.lookup("sub_v").unwrap().ty, Type::Vector(Box::new(Type::F64)));
        assert_eq!(env.lookup("m_elem").unwrap().ty, Type::F64);
        assert_eq!(env.lookup("m_sub").unwrap().ty, Type::matrix_dynamic(Type::F64));
    }

    #[test]
    fn test_typecheck_matrix_dimensions_success() {
        let code = r#"
            let A = mat [ 1.0, 2.0, 3.0 ; 4.0, 5.0, 6.0 ]; // 2x3
            let B = mat [ 1.0, 2.0 ; 3.0, 4.0 ; 5.0, 6.0 ]; // 3x2
            let C = A * B; // 2x2
        "#;
        let program = parse(code).expect("syntax ok");
        let env = check(&program, "test.gh", code).expect("type check ok");
        assert_eq!(env.lookup("C").unwrap().ty, Type::matrix(Type::F64, 2, 2));
    }

    #[test]
    fn test_typecheck_matrix_dimensions_mismatch_rejected() {
        let code = r#"
            let A = mat [ 1.0, 2.0, 3.0 ; 4.0, 5.0, 6.0 ]; // 2x3
            let B = mat [ 1.0, 2.0 ; 3.0, 4.0 ]; // 2x2
            let C = A * B;
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_err(), "Matrix dimension mismatch (2x3 * 2x2) must be rejected at compile time");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0102"));
    }

    #[test]
    fn test_typecheck_matrix_type_annotation_with_dims() {
        let code = r#"
            let A: Matrix[f64, 2, 2] = mat [ 1.0, 2.0 ; 3.0, 4.0 ];
        "#;
        let program = parse(code).expect("syntax ok");
        let env = check(&program, "test.gh", code).expect("type check ok");
        assert_eq!(env.lookup("A").unwrap().ty, Type::matrix(Type::F64, 2, 2));

        let bad_code = r#"
            let A: Matrix[f64, 3, 3] = mat [ 1.0, 2.0 ; 3.0, 4.0 ];
        "#;
        let bad_program = parse(bad_code).expect("syntax ok");
        let res = check(&bad_program, "test.gh", bad_code);
        assert!(res.is_err(), "Annotation dimension mismatch must be rejected");
        let diags = res.unwrap_err();
        assert!(diags.iter().any(|d| d.code == "C0102"));
    }

    #[test]
    fn test_typecheck_struct_trait_and_impl() {
        let code = r#"
            struct NormalDistribution {
                mean: f64,
                std_dev: f64,
            }

            trait Distribution {
                type Output;
                fn log_pdf(&self, x: Self::Output) -> f64;
            }

            impl Distribution for NormalDistribution {
                type Output = f64;
                fn log_pdf(&self, x: f64) -> f64 {
                    let diff = (x - self.mean) / self.std_dev;
                    -0.5 * diff * diff
                }
            }

            let dist = NormalDistribution { mean: 0.0, std_dev: 1.0 };
            let val = dist.log_pdf(0.5);
        "#;
        let program = parse(code).expect("syntax ok");
        let env = check(&program, "test.gh", code).expect("typecheck ok");
        assert_eq!(env.lookup("val").unwrap().ty, Type::F64);
    }

    #[test]
    fn test_typecheck_reject_missing_struct_field() {
        let code = r#"
            struct Point {
                x: f64,
                y: f64,
            }

            let p = Point { x: 1.0, z: 2.0 };
        "#;
        let program = parse(code).expect("syntax ok");
        let res = check(&program, "test.gh", code);
        assert!(res.is_err());
    }
}
