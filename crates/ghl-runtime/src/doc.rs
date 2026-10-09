use ghl_diagnostics::{CockpitPanel, RenderCaps};

/// Documentation record for a GHL built-in function or statistical verb.
#[derive(Debug, Clone)]
pub struct FunctionDoc {
    pub name: &'static str,
    pub signature: &'static str,
    pub formula: Option<&'static str>,
    pub summary: &'static str,
    pub description: &'static str,
    pub parameters: &'static [(&'static str, &'static str)],
    pub returns: &'static str,
    pub example: &'static str,
}

impl FunctionDoc {
    /// Renders the documentation as a Cockpit panel styled with ANSI colors.
    pub fn render(&self, caps: &RenderCaps) -> String {
        let mut panel = CockpitPanel::new(format!("Function: {}()", self.name));
        panel.with_badge("STDLIB");
        panel.add_line(self.summary);
        panel.add_divider();

        panel.add_kv("Signature", self.signature);
        if let Some(f) = self.formula {
            panel.add_kv("Formula", caps.bold(f));
        }
        panel.add_kv("Returns", self.returns);

        if !self.description.is_empty() {
            panel.add_divider();
            panel.add_line(self.description);
        }

        if !self.parameters.is_empty() {
            panel.add_divider();
            panel.add_line(caps.dim("Parameters:"));
            for (param, desc) in self.parameters {
                panel.add_kv(format!("  {param}"), *desc);
            }
        }

        if !self.example.is_empty() {
            panel.add_divider();
            panel.add_line(caps.dim("Example:"));
            for line in self.example.lines() {
                panel.add_line(format!("  {}", caps.cyan(line)));
            }
        }

        panel.render(caps)
    }
}

/// Look up documentation for a standard library function by name.
pub fn lookup_doc(name: &str) -> Option<&'static FunctionDoc> {
    DOCS.iter().find(|d| d.name == name)
}

/// Retrieve all documented functions.
pub fn all_docs() -> &'static [FunctionDoc] {
    &DOCS
}

static DOCS: [FunctionDoc; 44] = [
    // 1. Descriptive Statistics
    FunctionDoc {
        name: "mean",
        signature: "mean(x: Vector[T]) -> f64",
        formula: Some("x̄ = (1/n) Σ_{i=1}^n x_i"),
        summary: "Sample arithmetic mean with Kleene NA propagation",
        description: "Computes the sample mean of a numeric vector or in a dataframe summarize context. Propagates NA reasons under Kleene 3-valued logic.",
        parameters: &[(
            "x",
            "Numeric vector or column reference (in summarize/mutate)",
        )],
        returns: "f64 scalar or NA with reason",
        example: "let v = [1.0, 2.0, 3.0, 4.0];\nmean(v); // => 2.5",
    },
    FunctionDoc {
        name: "median",
        signature: "median(x: Vector[T]) -> f64",
        formula: Some("x̃ = x_{(n+1)/2} (odd) | (x_{n/2} + x_{n/2+1})/2 (even)"),
        summary: "Sample median (50th percentile)",
        description: "Returns the middle value of an ordered sample, or the average of the two middle values if n is even.",
        parameters: &[("x", "Numeric vector")],
        returns: "f64 scalar or NA",
        example: "let v = [1.0, 5.0, 2.0, 8.0, 7.0];\nmedian(v); // => 5.0",
    },
    FunctionDoc {
        name: "var",
        signature: "var(x: Vector[T]) -> f64",
        formula: Some("s² = (1/(n - 1)) Σ_{i=1}^n (x_i - x̄)²"),
        summary: "Sample variance (unbiased, N-1 degrees of freedom)",
        description: "Calculates the unbiased sample variance. Requires at least 2 non-NA observations.",
        parameters: &[("x", "Numeric vector with len >= 2")],
        returns: "f64 scalar or statistical warning if n < 2",
        example: "let v = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];\nvar(v); // => 4.5714...",
    },
    FunctionDoc {
        name: "std_dev",
        signature: "std_dev(x: Vector[T]) -> f64",
        formula: Some("s = √(var(x))"),
        summary: "Sample standard deviation",
        description: "Computes the square root of the sample variance.",
        parameters: &[("x", "Numeric vector with len >= 2")],
        returns: "f64 scalar",
        example: "let v = [10.0, 12.0, 23.0, 23.0, 16.0, 23.0, 21.0, 16.0];\nstd_dev(v);",
    },
    FunctionDoc {
        name: "sum",
        signature: "sum(x: Vector[T]) -> f64 | i64",
        formula: Some("S = Σ_{i=1}^n x_i"),
        summary: "Sum of all elements in a vector",
        description: "Returns the additive accumulation. An empty vector evaluates to 0.",
        parameters: &[("x", "Numeric vector")],
        returns: "f64 or i64 scalar",
        example: "sum([1, 2, 3, 4, 5]); // => 15",
    },
    FunctionDoc {
        name: "min",
        signature: "min(x: Vector[T]) -> T",
        formula: Some("min_{i} x_i"),
        summary: "Minimum element in a vector",
        description: "Returns the smallest non-missing value in the vector.",
        parameters: &[("x", "Numeric vector")],
        returns: "Scalar value of same type",
        example: "min([3.2, 1.1, 5.9]); // => 1.1",
    },
    FunctionDoc {
        name: "max",
        signature: "max(x: Vector[T]) -> T",
        formula: Some("max_{i} x_i"),
        summary: "Maximum element in a vector",
        description: "Returns the largest non-missing value in the vector.",
        parameters: &[("x", "Numeric vector")],
        returns: "Scalar value of same type",
        example: "max([3.2, 1.1, 5.9]); // => 5.9",
    },
    // 2. Statistical Modeling (NEKO Framework)
    FunctionDoc {
        name: "ols",
        signature: "ols(formula: Formula, data: DataFrame) -> ModelFit",
        formula: Some("y = Xβ + ε,  β̂ = (XᵀX)⁻¹ Xᵀy"),
        summary: "Ordinary Least Squares linear regression model",
        description: "Fits a Gaussian linear model using QR decomposition. Produces standard errors, t-statistics, p-values, R², adjusted R², AIC, and BIC.",
        parameters: &[
            ("formula", "Model specification (e.g. y ~ x1 + x2)"),
            ("data", "DataFrame containing the formula variables"),
        ],
        returns: "ModelFit object suitable for summary(), predict(), residuals()",
        example: "let fit = ols(mpg ~ wt + hp, mtcars);\nsummary(fit);",
    },
    FunctionDoc {
        name: "fit",
        signature: "fit(formula: Formula, data: DataFrame) -> ModelFit",
        formula: Some("y = Xβ + ε,  β̂ = (XᵀX)⁻¹ Xᵀy"),
        summary: "Alias for ols()",
        description: "Standard model fitting verb conforming to NEKO statistical conventions.",
        parameters: &[
            ("formula", "Model formula expression"),
            ("data", "Input DataFrame"),
        ],
        returns: "ModelFit object",
        example: "let model = fit(sales ~ ads + price, df);\nsummary(model);",
    },
    FunctionDoc {
        name: "fit_logistic",
        signature: "fit_logistic(formula: Formula, data: DataFrame) -> ModelFit",
        formula: Some("logit(p) = ln(p / (1 - p)) = Xβ,  p = 1 / (1 + e^(-Xβ))"),
        summary: "Binary Logistic Regression via IRLS (Iteratively Reweighted Least Squares)",
        description: "Fits a generalized linear model with binomial family and logit link function.",
        parameters: &[
            (
                "formula",
                "Binary response formula (e.g. churn ~ age + tenure)",
            ),
            ("data", "Input DataFrame"),
        ],
        returns: "Fitted GLM ModelFit object",
        example: "let logit_fit = fit_logistic(admit ~ gre + gpa, df);\nsummary(logit_fit);",
    },
    FunctionDoc {
        name: "fit_gmm",
        signature: "fit_gmm(data: DataFrame | Matrix, k: i64) -> ModelFit",
        formula: Some("p(x) = Σ_{k=1}^K π_k 𝒩(x | μ_k, Σ_k)"),
        summary: "Gaussian Mixture Model via Expectation-Maximization (EM)",
        description: "Performs unsupervised clustering and density estimation over k Gaussian components.",
        parameters: &[
            ("data", "DataFrame or numeric Matrix of observations"),
            ("k", "Number of mixture components (clusters)"),
        ],
        returns: "Fitted GMM ModelFit object",
        example: "let gmm_fit = fit_gmm(faithful, 2);\nsummary(gmm_fit);",
    },
    FunctionDoc {
        name: "summary",
        signature: "summary(model: ModelFit) -> ()",
        formula: Some("t_j = β̂_j / SE(β̂_j),  p_j = 2 * (1 - Φ(|t_j|))"),
        summary: "Prints a rich Cockpit diagnostics panel for a fitted model",
        description: "Displays model coefficients, standard errors, test statistics, confidence intervals, and goodness-of-fit metrics (R², AIC, BIC, F-stat).",
        parameters: &[(
            "model",
            "Fitted model from ols(), fit_logistic(), or fit_gmm()",
        )],
        returns: "Unit ()",
        example: "let fit = ols(y ~ x, df);\nsummary(fit);",
    },
    FunctionDoc {
        name: "predict",
        signature: "predict(model: ModelFit, new_data: DataFrame) -> Vector[f64]",
        formula: Some("ŷ = X β̂"),
        summary: "Computes predictions from a fitted statistical model",
        description: "Applies estimated parameters to new design matrix rows.",
        parameters: &[
            ("model", "Fitted ModelFit object"),
            ("new_data", "DataFrame containing the predictor variables"),
        ],
        returns: "Vector[f64] of predicted values",
        example: "let preds = predict(fit, test_df);",
    },
    FunctionDoc {
        name: "residuals",
        signature: "residuals(model: ModelFit) -> Vector[f64]",
        formula: Some("e_i = y_i - ŷ_i"),
        summary: "Extracts response residuals from a fitted model",
        description: "Returns the difference between observed response and model fitted values.",
        parameters: &[("model", "Fitted ModelFit object")],
        returns: "Vector[f64] of residuals",
        example: "let res = residuals(fit);\nplot(res) |> geom_histogram();",
    },
    FunctionDoc {
        name: "coef",
        signature: "coef(model: ModelFit) -> Vector[f64]",
        formula: Some("β̂ = [β̂_0, β̂_1, ..., β̂_p]ᵀ"),
        summary: "Extracts estimated coefficients vector",
        description: "Returns point estimates for intercept and predictors.",
        parameters: &[("model", "Fitted ModelFit object")],
        returns: "Vector[f64]",
        example: "let betas = coef(fit);",
    },
    FunctionDoc {
        name: "vcov",
        signature: "vcov(model: ModelFit) -> Matrix",
        formula: Some("Var(β̂) = s² (XᵀX)⁻¹"),
        summary: "Variance-covariance matrix of parameter estimates",
        description: "Returns the estimated covariance matrix for regression coefficients.",
        parameters: &[("model", "Fitted ModelFit object")],
        returns: "p x p numeric Matrix",
        example: "let cov_matrix = vcov(fit);",
    },
    // 3. Linear Algebra & Vector Mathematics
    FunctionDoc {
        name: "dot",
        signature: "dot(a: Vector[f64], b: Vector[f64]) -> f64",
        formula: Some("a · b = Σ_{i=1}^n a_i b_i"),
        summary: "Inner (dot) product of two numeric vectors",
        description: "Computes the inner product with SIMD vectorization backed by faer.",
        parameters: &[
            ("a", "First numeric vector"),
            ("b", "Second numeric vector of identical length"),
        ],
        returns: "f64 scalar",
        example: "let a = [1.0, 2.0, 3.0];\nlet b = [4.0, 5.0, 6.0];\ndot(a, b); // => 32.0",
    },
    FunctionDoc {
        name: "transpose",
        signature: "transpose(A: Matrix) -> Matrix",
        formula: Some("(Aᵀ)_{i,j} = A_{j,i}"),
        summary: "Matrix transpose (alias: t)",
        description: "Reflects matrix elements across its main diagonal.",
        parameters: &[("A", "Matrix of dimensions m x n")],
        returns: "Transposed matrix of dimensions n x m",
        example: "let A = mat [1.0, 2.0 ; 3.0, 4.0];\ntranspose(A);",
    },
    FunctionDoc {
        name: "cholesky",
        signature: "cholesky(A: Matrix) -> Matrix",
        formula: Some("A = L Lᵀ  (L is lower triangular)"),
        summary: "Cholesky decomposition of symmetric positive-definite matrix",
        description: "Computes the unique lower-triangular matrix L such that A = L Lᵀ.",
        parameters: &[("A", "Symmetric positive-definite Matrix")],
        returns: "Lower-triangular Matrix L",
        example: "let A = mat [4.0, 2.0 ; 2.0, 3.0];\nlet L = cholesky(A);",
    },
    FunctionDoc {
        name: "qr",
        signature: "qr(A: Matrix) -> QrDecomp",
        formula: Some("A = Q R  (Q orthogonal, R upper triangular)"),
        summary: "Thin QR matrix decomposition",
        description: "Factorizes matrix A into orthogonal Q and upper triangular R.",
        parameters: &[("A", "Matrix of dimensions m x n (m >= n)")],
        returns: "Decomposition object accessible via qr_q() and qr_r()",
        example: "let decomp = qr(A);\nlet Q = qr_q(decomp);\nlet R = qr_r(decomp);",
    },
    FunctionDoc {
        name: "svd",
        signature: "svd(A: Matrix) -> SvdDecomp",
        formula: Some("A = U Σ Vᵀ"),
        summary: "Singular Value Decomposition",
        description: "Computes left singular vectors U, singular values Σ, and right singular vectors V.",
        parameters: &[("A", "Matrix")],
        returns: "SvdDecomp accessible via svd_u(), svd_s(), svd_v()",
        example: "let s = svd(A);\nlet u = svd_u(s);\nlet vals = svd_s(s);",
    },
    FunctionDoc {
        name: "eigen",
        signature: "eigen(A: Matrix) -> EigenDecomp",
        formula: Some("A v = λ v"),
        summary: "Spectral decomposition for symmetric matrices",
        description: "Computes real eigenvalues and eigenvectors for symmetric matrices.",
        parameters: &[("A", "Symmetric square Matrix")],
        returns: "EigenDecomp accessible via eigen_values() and eigen_vectors()",
        example: "let e = eigen(A);\nlet vals = eigen_values(e);",
    },
    FunctionDoc {
        name: "identity",
        signature: "identity(n: i64) -> Matrix",
        formula: Some("I_n = diag(1, 1, ..., 1)"),
        summary: "Generate n x n identity matrix (alias: eye)",
        description: "Constructs an n x n square matrix with ones on the main diagonal and zeros elsewhere.",
        parameters: &[("n", "Dimension of the square matrix")],
        returns: "n x n Matrix",
        example: "identity(3); // 3x3 identity matrix",
    },
    FunctionDoc {
        name: "diag",
        signature: "diag(x: Vector[f64] | Matrix) -> Matrix | Vector[f64]",
        formula: Some("diag(v)_{i,i} = v_i,  diag(M)_i = M_{i,i}"),
        summary: "Constructs diagonal matrix from vector, or extracts diagonal of matrix",
        description: "Dual-purpose diagonal operator matching standard mathematical notation.",
        parameters: &[(
            "x",
            "Numeric Vector to expand or Matrix to extract diagonal from",
        )],
        returns: "Matrix or Vector[f64]",
        example: "diag([1.0, 2.0, 3.0]);",
    },
    // 4. Data Wrangling (Tidyverse Verbs)
    FunctionDoc {
        name: "filter",
        signature: "filter(df: DataFrame, condition: Expr) -> DataFrame",
        formula: None,
        summary: "Subset rows using column predicate expressions",
        description: "Filters rows where the condition evaluates to true under Kleene logic.",
        parameters: &[
            ("df", "Input DataFrame"),
            (
                "condition",
                "Boolean expression over column names (e.g. age > 30)",
            ),
        ],
        returns: "Filtered DataFrame",
        example: "df |> filter(price > 100.0 && category == \"Tech\");",
    },
    FunctionDoc {
        name: "select",
        signature: "select(df: DataFrame, col1, col2, ...) -> DataFrame",
        formula: None,
        summary: "Keep only specified columns from a DataFrame",
        description: "Subsets tabular data by column names in the requested order.",
        parameters: &[
            ("df", "Input DataFrame"),
            ("cols", "One or more column identifiers"),
        ],
        returns: "DataFrame with selected columns",
        example: "df |> select(id, name, salary);",
    },
    FunctionDoc {
        name: "mutate",
        signature: "mutate(df: DataFrame, col_name = expr) -> DataFrame",
        formula: None,
        summary: "Add new columns or transform existing columns",
        description: "Computes vectorized expressions and attaches results to the DataFrame.",
        parameters: &[
            ("df", "Input DataFrame"),
            ("col_name = expr", "Column assignment expression"),
        ],
        returns: "DataFrame with updated columns",
        example: "df |> mutate(log_income = log(income));",
    },
    FunctionDoc {
        name: "group_by",
        signature: "group_by(df: DataFrame, col1, [col2, ...]) -> DataFrame",
        formula: None,
        summary: "Group DataFrame by categorical keys for aggregate operations",
        description: "Pairs with summarize() to compute per-group statistics.",
        parameters: &[
            ("df", "Input DataFrame"),
            ("cols", "Grouping column identifiers"),
        ],
        returns: "Grouped DataFrame",
        example: "df |> group_by(department) |> summarize(avg_salary = mean(salary));",
    },
    FunctionDoc {
        name: "summarize",
        signature: "summarize(df: DataFrame, stat = agg_fn(col)) -> DataFrame",
        formula: None,
        summary: "Aggregate groups or full dataset into summary statistics",
        description: "Collapses rows into aggregate metrics (mean, sum, count, min, max, etc.).",
        parameters: &[
            ("df", "Grouped or ungrouped DataFrame"),
            ("aggregations", "Named aggregation expressions"),
        ],
        returns: "Summary DataFrame",
        example: "df |> group_by(region) |> summarize(n = count(), total = sum(sales));",
    },
    FunctionDoc {
        name: "arrange",
        signature: "arrange(df: DataFrame, col | desc(col)) -> DataFrame",
        formula: None,
        summary: "Sort rows by one or more columns",
        description: "Reorders rows in ascending or descending order.",
        parameters: &[
            ("df", "Input DataFrame"),
            ("cols", "Sort keys (use desc(col) for descending)"),
        ],
        returns: "Sorted DataFrame",
        example: "df |> arrange(desc(score));",
    },
    // 5. Grammar of Graphics
    FunctionDoc {
        name: "plot",
        signature: "plot(data: DataFrame, mapping: aes) -> PlotSpec",
        formula: None,
        summary: "Initialize Grammar of Graphics plot specification (RFC 09)",
        description: "Sets the baseline dataset and aesthetic mappings for visualization.",
        parameters: &[
            ("data", "DataFrame containing plotting variables"),
            (
                "mapping",
                "Aesthetic mapping constructed with aes(x: ..., y: ...)",
            ),
        ],
        returns: "PlotSpec pipeline object",
        example: "plot(df, aes(x: \"wt\", y: \"mpg\")) |> geom_point() |> show();",
    },
    FunctionDoc {
        name: "aes",
        signature: "aes(x: String, [y: String, color: String]) -> AestheticMap",
        formula: None,
        summary: "Construct aesthetic mappings between data columns and visual properties",
        description: "Defines visual roles (x, y, color, size) for plot geometry layers.",
        parameters: &[
            ("x", "X-axis column name"),
            ("y", "Optional Y-axis column name"),
            ("color", "Optional grouping/color column name"),
        ],
        returns: "Aesthetic mapping object",
        example: "aes(x: \"displacement\", y: \"horsepower\", color: \"cylinders\")",
    },
    // 6. Environment & Utility
    FunctionDoc {
        name: "rm",
        signature: "rm(name1: String, [name2: String, ...]) -> ()",
        formula: Some("env.remove(name)"),
        summary: "Remove variables from the active session environment",
        description: "Deletes one or more user variables, releasing their memory and unbinding their symbols. In REPL, you can also use `:rm <name>` or `:clear-vars`.",
        parameters: &[(
            "names",
            "Names of the variables to remove as strings or references",
        )],
        returns: "Unit ()",
        example: "rm(\"temp_df\");\nrm(\"x\", \"y\", \"fit\");",
    },
    FunctionDoc {
        name: "help",
        signature: "help([function_name: String]) -> ()",
        formula: None,
        summary: "Display documentation and mathematical formulas for GHL functions (alias: doc)",
        description: "Prints comprehensive documentation cards with signatures, formulas, and examples. In REPL, `?<name>` or entering just the function name achieves the same.",
        parameters: &[("function_name", "Optional name of the function to inspect")],
        returns: "Unit ()",
        example: "help(\"ols\");\nhelp(\"mean\");",
    },
    FunctionDoc {
        name: "map",
        signature: "map(v: Vector[T], fn: Function) -> Vector[U]",
        formula: Some("y_i = f(v_i)"),
        summary: "Vectorized elementwise application of a function or lambda",
        description: "Applies a closure or native function to each element of a vector, returning a new vector.",
        parameters: &[
            ("v", "Input vector"),
            ("fn", "Unary function or lambda \\x -> ..."),
        ],
        returns: "Transformed Vector",
        example: "let v = [1.0, 2.0, 3.0];\nv |> map(\\x -> x * 2.0 + 1.0);",
    },
    FunctionDoc {
        name: "view",
        signature: "view(df: DataFrame) -> String",
        formula: None,
        summary: "Export and open DataFrame in Positron Data Explorer (alias: View)",
        description: "Serializes the DataFrame to a zero-copy Parquet file and launches Positron's interactive data grid with sorting and filtering.",
        parameters: &[("df", "Input DataFrame to inspect interactively")],
        returns: "Temporary file path string",
        example: "df |> view();",
    },
    FunctionDoc {
        name: "show",
        signature: "show(target: Plot | Value) -> ()",
        formula: None,
        summary: "Render plot or value to terminal Cockpit Deck and Positron Plots Pane",
        description: "Outputs an ASCII/Unicode visualization to stdout, and exports an SVG vector figure when running inside Positron IDE.",
        parameters: &[("target", "PlotSpec or value to display")],
        returns: "Unit ()",
        example: "plot(df, aes(\"x\", \"y\")) |> geom_point() |> show();",
    },
    FunctionDoc {
        name: "theme_minimal",
        signature: "theme_minimal([plot: Plot]) -> Plot",
        formula: None,
        summary: "Apply a clean, minimal plot aesthetic theme",
        description: "Sets light background with subtle dashed gridlines and clean typography.",
        parameters: &[("plot", "Plot pipeline object")],
        returns: "Plot",
        example: "plot(df, aes(\"x\", \"y\")) |> geom_point() |> theme_minimal();",
    },
    FunctionDoc {
        name: "theme_classic",
        signature: "theme_classic([plot: Plot]) -> Plot",
        formula: None,
        summary: "Apply a classic publication-ready plot aesthetic theme",
        description: "Sets pure white background with solid axis lines and no gridlines.",
        parameters: &[("plot", "Plot pipeline object")],
        returns: "Plot",
        example: "plot(df, aes(\"x\", \"y\")) |> geom_point() |> theme_classic();",
    },
    FunctionDoc {
        name: "theme_dark",
        signature: "theme_dark([plot: Plot]) -> Plot",
        formula: None,
        summary: "Apply a high-contrast dark mode plot aesthetic theme",
        description: "Sets dark background with neon palette accents and subtle gridlines.",
        parameters: &[("plot", "Plot pipeline object")],
        returns: "Plot",
        example: "plot(df, aes(\"x\", \"y\")) |> geom_point() |> theme_dark();",
    },
    FunctionDoc {
        name: "theme_apa",
        signature: "theme_apa([plot: Plot]) -> Plot",
        formula: None,
        summary: "Apply APA 7th edition publication-quality plot aesthetic theme",
        description: "Sets pure white background, crisp solid axes, borderless legend, and eliminates minor gridlines.",
        parameters: &[("plot", "Plot pipeline object")],
        returns: "Plot",
        example: "plot(df, aes(\"x\", \"y\")) |> geom_point() |> theme_apa();",
    },
    FunctionDoc {
        name: "factor",
        signature: "factor(x: Vector[T], [levels: Vector[String]], [contrast: String]) -> Factor",
        formula: None,
        summary: "Construct a categorical factor vector with contrast coding",
        description: "Encodes discrete levels with contrast schemes (treatment, sum, helmert, poly) for statistical modeling.",
        parameters: &[
            ("x", "Input vector"),
            ("levels", "Optional explicit level ordering"),
            (
                "contrast",
                "Contrast scheme: 'treatment', 'sum', 'helmert', or 'poly'",
            ),
        ],
        returns: "Factor",
        example: "let f = factor([\"ctrl\", \"trt1\", \"trt2\"]);",
    },
    FunctionDoc {
        name: "ordered_factor",
        signature: "ordered_factor(x: Vector[T], [levels: Vector[String]], [contrast: String]) -> Factor",
        formula: None,
        summary: "Construct an ordinal factor vector with polynomial contrasts",
        description: "Encodes ranked discrete levels defaulting to orthogonal polynomial contrasts (linear, quadratic, etc.).",
        parameters: &[
            ("x", "Input vector"),
            ("levels", "Optional explicit ordered levels"),
            ("contrast", "Contrast scheme (defaults to 'poly')"),
        ],
        returns: "Factor (ordered)",
        example: "let ord = ordered_factor([\"low\", \"med\", \"high\"]);",
    },
    FunctionDoc {
        name: "levels",
        signature: "levels(x: Factor) -> Vector[String]",
        formula: None,
        summary: "Extract the unique discrete levels of a factor",
        description: "Returns a vector containing the unique levels in order.",
        parameters: &[("x", "Factor vector")],
        returns: "Vector[String]",
        example: "levels(factor([\"a\", \"b\", \"a\"]));",
    },
];
