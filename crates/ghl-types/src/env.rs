use std::collections::HashMap;
use crate::ContrastScheme;
use crate::types::Type;

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolInfo {
    pub ty: Type,
    pub is_mut: bool,
}

#[derive(Debug, Clone)]
pub struct TypeEnv {
    scopes: Vec<HashMap<String, SymbolInfo>>,
    pub structs: HashMap<String, Vec<(String, Type)>>,
    pub traits: HashMap<String, HashMap<String, (Vec<Type>, Type)>>,
    pub impl_methods: HashMap<(String, String), (Vec<Type>, Type)>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            structs: HashMap::new(),
            traits: HashMap::new(),
            impl_methods: HashMap::new(),
        }
    }

    pub fn with_prelude() -> Self {
        let mut env = Self::new();

        // Register Standard Library Statistics
        env.insert(
            "mean".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "median".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "sum".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "var".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "std_dev".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "min".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "max".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::Any),
            },
            false,
        );

        // Register DataFrame verbs
        env.insert(
            "col".into(),
            Type::Function {
                params: vec![Type::String],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "filter".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "select".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "mutate".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "arrange".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "group_by".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "summarize".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "pivot_wider".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "pivot_longer".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "impute".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "filter_na_reason".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );

        // Imputation strategies (RFC 01 §7.3 & RFC 02 §2.2)
        env.insert("Mean".into(), Type::String, false);
        env.insert("Median".into(), Type::String, false);
        env.insert("Mode".into(), Type::String, false);
        env.insert("Const".into(), Type::String, false);

        // Register Linear Algebra
        env.insert(
            "inv".into(),
            Type::Function {
                params: vec![Type::matrix_dynamic(Type::F64)],
                ret: Box::new(Type::matrix_dynamic(Type::F64)),
            },
            false,
        );
        env.insert(
            "transpose".into(),
            Type::Function {
                params: vec![Type::matrix_dynamic(Type::Any)],
                ret: Box::new(Type::matrix_dynamic(Type::Any)),
            },
            false,
        );
        env.insert(
            "det".into(),
            Type::Function {
                params: vec![Type::matrix_dynamic(Type::F64)],
                ret: Box::new(Type::F64),
            },
            false,
        );

        // Mascot macros/helpers
        env.insert(
            "purr".into(),
            Type::Function {
                params: vec![Type::String],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "pounce".into(),
            Type::Function {
                params: vec![Type::Bool, Type::String],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "clock_now".into(),
            Type::Function {
                params: vec![],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "set_telemetry".into(),
            Type::Function {
                params: vec![Type::Bool],
                ret: Box::new(Type::Bool),
            },
            false,
        );

        // Print helpers
        env.insert(
            "println".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "to_string".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::String),
            },
            false,
        );
        env.insert(
            "to_int".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::I64),
            },
            false,
        );
        env.insert(
            "int".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::I64),
            },
            false,
        );
        env.insert(
            "to_float".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "float".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "to_bool".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Bool),
            },
            false,
        );
        env.insert(
            "bool".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Bool),
            },
            false,
        );
        env.insert(
            "format".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::String),
            },
            false,
        );
        env.insert(
            "panic".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "sha256".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::String),
            },
            false,
        );
        env.insert(
            "length".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::I64),
            },
            false,
        );
        env.insert(
            "filter_na".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Vector(Box::new(Type::Any))),
            },
            false,
        );
        env.insert(
            "quantile".into(),
            Type::Function {
                params: vec![Type::Any, Type::Any],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "append".into(),
            Type::Function {
                params: vec![Type::Any, Type::Any],
                ret: Box::new(Type::Vector(Box::new(Type::Any))),
            },
            false,
        );
        env.insert(
            "print".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "rm".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "help".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "doc".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );

        // NEKO Statistical Modeling Verbs
        env.insert(
            "fit".into(),
            Type::Function {
                params: vec![Type::Formula, Type::Any],
                ret: Box::new(Type::ModelFit),
            },
            false,
        );
        env.insert(
            "ols".into(),
            Type::Function {
                params: vec![Type::Formula, Type::Any],
                ret: Box::new(Type::ModelFit),
            },
            false,
        );
        env.insert(
            "fit_logistic".into(),
            Type::Function {
                params: vec![Type::Formula, Type::Any],
                ret: Box::new(Type::ModelFit),
            },
            false,
        );
        env.insert(
            "logistic".into(),
            Type::Function {
                params: vec![Type::Formula, Type::Any],
                ret: Box::new(Type::ModelFit),
            },
            false,
        );
        env.insert(
            "fit_poisson".into(),
            Type::Function {
                params: vec![Type::Formula, Type::Any],
                ret: Box::new(Type::ModelFit),
            },
            false,
        );
        env.insert(
            "poisson".into(),
            Type::Function {
                params: vec![Type::Formula, Type::Any],
                ret: Box::new(Type::ModelFit),
            },
            false,
        );
        env.insert(
            "fit_gmm".into(),
            Type::Function {
                params: vec![Type::Any, Type::I64],
                ret: Box::new(Type::ModelFit),
            },
            false,
        );
        env.insert(
            "gmm".into(),
            Type::Function {
                params: vec![Type::Any, Type::I64],
                ret: Box::new(Type::ModelFit),
            },
            false,
        );
        env.insert(
            "summary".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "tidy".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "glance".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "augment".into(),
            Type::Function {
                params: vec![Type::Any, Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "residuals".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Vector(Box::new(Type::F64))),
            },
            false,
        );
        env.insert(
            "predict".into(),
            Type::Function {
                params: vec![Type::Any, Type::Any],
                ret: Box::new(Type::Vector(Box::new(Type::F64))),
            },
            false,
        );
        env.insert(
            "coef".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Vector(Box::new(Type::F64))),
            },
            false,
        );
        env.insert(
            "vcov".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::matrix_dynamic(Type::F64)),
            },
            false,
        );

        // Grammar of Graphics (RFC 09) Verbs
        env.insert(
            "plot".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "aes".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "geom_point".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "geom_line".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "geom_smooth".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "geom_histogram".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "geom_boxplot".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "geom_bar".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "labs".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "show".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "save".into(),
            Type::Function {
                params: vec![Type::Any, Type::String],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "scatter".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "hist".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "histogram".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "boxplot".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "theme_minimal".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "theme_classic".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "theme_dark".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );

        // Factor & Categorical primitives
        env.insert(
            "factor".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Factor {
                    levels: Vec::new(),
                    ordered: false,
                    contrast: ContrastScheme::Treatment,
                }),
            },
            false,
        );
        env.insert(
            "ordered_factor".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Factor {
                    levels: Vec::new(),
                    ordered: true,
                    contrast: ContrastScheme::Polynomial,
                }),
            },
            false,
        );
        env.insert(
            "levels".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Vector(Box::new(Type::String))),
            },
            false,
        );

        // Primitive and Tabular I/O
        env.insert(
            "read_file".into(),
            Type::Function {
                params: vec![Type::String],
                ret: Box::new(Type::String),
            },
            false,
        );
        env.insert(
            "read_lines".into(),
            Type::Function {
                params: vec![Type::String],
                ret: Box::new(Type::Vector(Box::new(Type::String))),
            },
            false,
        );
        env.insert(
            "write_file".into(),
            Type::Function {
                params: vec![Type::Any, Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "append_file".into(),
            Type::Function {
                params: vec![Type::Any, Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "file_exists".into(),
            Type::Function {
                params: vec![Type::String],
                ret: Box::new(Type::Bool),
            },
            false,
        );
        env.insert(
            "read_csv".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "parse_csv".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "write_csv".into(),
            Type::Function {
                params: vec![Type::Any, Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "read_parquet".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "write_parquet".into(),
            Type::Function {
                params: vec![Type::Any, Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "view".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::String),
            },
            false,
        );
        env.insert(
            "View".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::String),
            },
            false,
        );
        env.insert(
            "head".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "tail".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );

        // Extended DataFrame Wrangling Verbs
        env.insert(
            "mutate".into(),
            Type::Function {
                params: vec![Type::Any, Type::String, Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "arrange".into(),
            Type::Function {
                params: vec![Type::Any, Type::String],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "rename".into(),
            Type::Function {
                params: vec![Type::Any, Type::String, Type::String],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "drop".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "distinct".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "nrow".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::I64),
            },
            false,
        );
        env.insert(
            "ncol".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::I64),
            },
            false,
        );
        env.insert(
            "colnames".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Vector(Box::new(Type::String))),
            },
            false,
        );
        env.insert(
            "slice".into(),
            Type::Function {
                params: vec![Type::Any, Type::I64, Type::I64],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );

        // Grouping / summarizing / sorting / row-selection helpers / joins
        for name in [
            "ungroup", "first", "last", "n_distinct", "count", "coalesce", "desc",
            "pull", "fill_na", "fill_na_all", "glimpse", "slice_min", "slice_max",
            "sample_n", "sample_frac", "inner_join", "left_join",
            "na_reason", "na_reasons", "is_na", "is_vector",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        // Statistical distributions & testing (RFC 15 Capa 0 Kernel + Capa 1 Stdlib)
        for name in [
            "t_pdf", "t_cdf", "t_quantile", "random_t",
            "f_pdf", "f_cdf", "f_quantile", "random_f",
            "chisq_pdf", "chisq_cdf", "chisq_quantile", "random_chisq",
            "beta_pdf", "beta_cdf", "beta_quantile", "random_beta",
            "uniform_pdf", "uniform_cdf", "uniform_quantile",
            "exponential_pdf", "exponential_cdf", "exponential_quantile", "random_exponential",
            "binomial_pmf", "binomial_cdf", "binomial_quantile", "random_binomial",
            "poisson_pmf", "poisson_cdf", "poisson_quantile", "random_poisson",
            "normal_quantile", "gamma_quantile",
            "feols", "iv_regress", "ridge", "lasso", "elastic_net",
            "irt_em_quadrature_kernel", "irt_quadrature_grid",
            "sigmoid_matmul", "sigmoid", "log_sum_exp", "softmax", "fused_mul_add",
            "row_sums", "col_sums", "row_means", "col_means", "row_maxs", "col_maxs", "row_mins", "col_mins",
            "sem", "decompose_spec", "optim", "sample_cov", "anova", "t_test", "t_test_one_sample", "chisq_test",
            "normal", "student_t", "fisher_f", "chisq", "gamma_dist", "beta_dist",
            "uniform", "exponential", "binomial", "poisson_dist",
            "conf_int", "p_value", "z_test_one_sample", "cor_test",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        // Math helpers — scalar + Vector[f64]
        for name in ["ln", "log", "log2", "log10", "exp", "sqrt", "abs", "floor", "ceil", "round", "pow", "clamp", "sin", "cos"] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }
        env.insert("pi".into(), Type::F64, false);
        env.insert("e".into(), Type::F64, false);

        // Dense linear algebra (TODO.md Fase 3, faer-backed)
        for name in [
            "dot", "map", "random_uniform", "bootstrap_mean",
            "random_normal", "random_gamma", "normal_pdf", "normal_cdf", "gamma_pdf", "gamma_cdf",
            "qr", "qr_q", "qr_r", "cholesky",
            "svd", "svd_u", "svd_s", "svd_v",
            "eigen", "eigen_values", "eigen_vectors",
            "zeros", "len", "get", "set", "get_row", "set_row", "get_col",
            "transpose", "t", "identity", "eye", "diag", "log_sum_exp",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        // Vector / window helpers
        for name in [
            "cumsum", "cumprod", "cummax", "cummin", "lag", "lead",
            "if_else", "between", "sort_asc", "sort_desc", "rank",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        // String helpers — scalar String + Vector[String]
        for name in [
            "str_upper", "str_lower", "str_trim", "str_replace", "str_pad",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::String) },
                false,
            );
        }
        env.insert(
            "str_split".into(),
            Type::Function { params: vec![Type::Any, Type::Any], ret: Box::new(Type::Vector(Box::new(Type::String))) },
            false,
        );
        env.insert(
            "str_len".into(),
            Type::Function { params: vec![Type::Any], ret: Box::new(Type::I64) },
            false,
        );
        for name in ["str_contains", "str_starts", "str_ends"] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any, Type::Any], ret: Box::new(Type::Bool) },
                false,
            );
        }

        // Regional Memory Arenas (RFC 03 §2.2)
        for name in [
            "scope", "arena::scope", "alloc_vector", "arena::alloc_vector",
            "alloc_matrix", "arena::alloc_matrix", "reset", "arena::reset",
            "allocated_bytes", "arena::allocated_bytes",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        // Automatic Differentiation (RFC 04 §5)
        for name in [
            "grad", "autodiff::grad", "diff", "autodiff::diff",
            "value_and_grad", "autodiff::value_and_grad",
            "jacobian", "autodiff::jacobian",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        // Network & HTTP Microservices (RFC 04 §6)
        for name in [
            "http::serve", "http_serve", "http::response", "http_response",
            "http::get", "http_get", "http::post", "http_post",
            "net::tcp_connect", "tcp_connect",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        // Parallel Iterators (RFC 05 §2)
        for name in [
            "par_iter", "concurrency::par_iter",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        // GPU Acceleration & Shaders (RFC 05 §4)
        for name in [
            "Device", "gpu::Device",
            "GpuMatrix", "gpu::GpuMatrix",
            "GpuVector", "gpu::GpuVector",
            "PhiloxRng", "gpu::PhiloxRng",
            "gemm", "gpu::gemm",
            "reduce_sum", "gpu::reduce_sum",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        // Probability Distributions Subsystem (`std::prob` per RFC 15)
        env.insert_struct("Normal".into(), vec![("mean".into(), Type::F64), ("sd".into(), Type::F64)]);
        env.insert_struct("StudentT".into(), vec![("df".into(), Type::F64)]);
        env.insert_struct("FisherF".into(), vec![("df1".into(), Type::F64), ("df2".into(), Type::F64)]);
        env.insert_struct("ChiSq".into(), vec![("df".into(), Type::F64)]);
        env.insert_struct("Gamma".into(), vec![("shape".into(), Type::F64), ("rate".into(), Type::F64)]);
        env.insert_struct("Beta".into(), vec![("alpha".into(), Type::F64), ("beta".into(), Type::F64)]);
        env.insert_struct("Binomial".into(), vec![("n".into(), Type::F64), ("p".into(), Type::F64)]);
        env.insert_struct("Poisson".into(), vec![("lambda".into(), Type::F64)]);
        env.insert_struct("Uniform".into(), vec![("min".into(), Type::F64), ("max".into(), Type::F64)]);
        env.insert_struct("Exponential".into(), vec![("rate".into(), Type::F64)]);

        for name in [
            "p_value", "conf_int", "z_test_one_sample", "cor_test",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        // Cockpit Deck Subsystem (RFC 14)
        for name in [
            "cockpit", "cockpit_with_badge", "cockpit_add_kv", "cockpit_add_line",
            "cockpit_add_divider", "render_cockpit", "show_cockpit", "cockpit_render",
            "format_cockpit", "cockpit_format", "sparkline", "cockpit_sparkline",
        ] {
            env.insert(
                name.into(),
                Type::Function { params: vec![Type::Any], ret: Box::new(Type::Any) },
                false,
            );
        }

        env
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn insert(&mut self, name: String, ty: Type, is_mut: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, SymbolInfo { ty, is_mut });
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&SymbolInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    pub fn remove(&mut self, name: &str) -> Option<SymbolInfo> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.remove(name) {
                return Some(info);
            }
        }
        None
    }

    pub fn insert_struct(&mut self, name: String, fields: Vec<(String, Type)>) {
        self.structs.insert(name, fields);
    }

    pub fn lookup_struct(&self, name: &str) -> Option<&Vec<(String, Type)>> {
        self.structs.get(name)
    }

    pub fn insert_trait(&mut self, name: String, methods: HashMap<String, (Vec<Type>, Type)>) {
        self.traits.insert(name, methods);
    }

    pub fn lookup_trait(&self, name: &str) -> Option<&HashMap<String, (Vec<Type>, Type)>> {
        self.traits.get(name)
    }

    pub fn insert_impl_method(&mut self, struct_name: String, method_name: String, params: Vec<Type>, ret: Type) {
        self.impl_methods.insert((struct_name, method_name), (params, ret));
    }

    pub fn lookup_method(&self, struct_name: &str, method_name: &str) -> Option<&(Vec<Type>, Type)> {
        self.impl_methods.get(&(struct_name.to_string(), method_name.to_string()))
    }
}
