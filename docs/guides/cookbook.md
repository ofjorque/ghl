# The GHL Statistical Cookbook

Bienvenido a **The GHL Statistical Cookbook**. Este recetario reúne patrones y soluciones completas y reproducibles para problemas comunes y avanzados de computación estadística, limpieza de datos, modelado econométrico y aprendizaje probabilístico en **GHL** (*Generalized Hypothesis Language*).

---

## Índice de Recetas

1. [Receta 1: Limpieza e Imputación de Datos Faltantes con Motivos Semánticos](#receta-1-limpieza-e-imputación-de-datos-faltantes-con-motivos-semánticos)
2. [Receta 2: Regresión Lineal Robusta con Errores Estándar Heterocedásticos (HC0 a HC3)](#receta-2-regresión-lineal-robusta-con-errores-estándar-heterocedásticos-hc0-a-hc3)
3. [Receta 3: Clasificación Binaria con Regresión Logística (IRLS)](#receta-3-clasificación-binaria-con-regresión-logística-irls)
4. [Receta 4: Agrupamiento No Supervisado con Modelos de Mezclas Gaussianas (GMM / EM)](#receta-4-agrupamiento-no-supervisado-con-modelos-de-mezclas-gaussianas-gmm--em)
5. [Receta 5: Muestreo Bayesiano MCMC y Bootstrap Paralelo Reproducible](#receta-5-muestreo-bayesiano-mcmc-y-bootstrap-paralelo-reproducible)

---

## Receta 1: Limpieza e Imputación de Datos Faltantes con Motivos Semánticos

### Problema
En los conjuntos de datos del mundo real (registros médicos, encuestas, sensores IoT), los valores faltantes no son todos iguales. Un valor ausente porque el participante se negó a responder (`NA:NoResponse`) suele violar el supuesto de *Missing Completely at Random* (MCAR), mientras que una pérdida transitoria por desconexión de red (`NA:SensorDropout`) es puramente técnica y puede imputarse válidamente sin introducir sesgo sistemático.

### Solución en GHL
GHL permite etiquetar valores con motivos semánticos `NA:Reason`, auditar su presencia con `na_reasons()`, filtrar filas según el motivo con `filter_na_reason()` e imputar condicionalmente con `impute()`.

```ghl
// recipe_01_imputation.gh
// Clean and impute missing data using semantic NA reasons

// 1. Create a raw clinical trial dataset with distinct NA reasons
let raw_trials = dataframe {
    subject_id: [101, 102, 103, 104, 105, 106],
    age: [45.0, 52.0, 39.0, 61.0, 48.0, 55.0],
    income: [55000.0, NA:NoResponse, 62000.0, 71000.0, NA:NoResponse, 58000.0],
    systolic_bp: [122.0, 135.0, NA:SensorDropout, 140.0, 128.0, NA:SensorDropout],
    cholesterol: [195.0, 210.0, 188.0, NA:MeasurementFailure, 202.0, 215.0]
};

// 2. Audit existing NA reasons in the dataset
let audit_report = na_reasons(raw_trials);

// 3. Pipeline: 
//    a) Drop participants who refused to report income (MNAR risk)
//    b) Impute sensor dropouts in systolic_bp using the mean of valid observations
//    c) Impute measurement failures in cholesterol using the median
let clean_trials = raw_trials
    |> filter_na_reason(income, drop: [NAReason::NoResponse])
    |> impute(systolic_bp, strategy: Mean, only_for: [NAReason::SensorDropout])
    |> impute(cholesterol, strategy: Median, only_for: [NAReason::MeasurementFailure]);

// 4. Verify that the resulting dataset is complete and ready for estimation
let clean_n = len(clean_trials);
let avg_bp = mean(pull(clean_trials, "systolic_bp"));
```

---

## Receta 2: Regresión Lineal Robusta con Errores Estándar Heterocedásticos (HC0 a HC3)

### Problema
El modelo clásico de Mínimos Cuadrados Ordinarios (OLS) asume varianza constante en los errores ($\text{Var}(\varepsilon_i | X) = \sigma^2$, homocedasticidad). En presencia de heterocedasticidad (común en finanzas y econometría), los estimadores $\hat{\beta}$ siguen siendo insesgados, pero la matriz de covarianza clásica subestima severamente los errores estándar, produciendo valores $p$ artificialmente optimistas e inferencias espurias.

### Solución en GHL
GHL implementa la familia de estimadores sándwich consistentes ante heterocedasticidad de White/MacKinnon-White (**HC0**, **HC1**, **HC2**, **HC3**) como opciones nativas en la función `vcov()`.

$$\hat{\Sigma}_{\text{HC}} = (X^T X)^{-1} X^T \hat{\Omega} X (X^T X)^{-1}$$

- **HC0**: Estimador asintótico original de White ($\hat{\Omega} = \text{diag}(e_i^2)$).
- **HC1**: Corrección por grados de libertad ($\frac{n}{n - p} \text{HC0}$).
- **HC2**: Ponderación por apalancamiento (*leverage* $h_{ii}$): $e_i^2 / (1 - h_{ii})$.
- **HC3**: Ponderación cuadrática recomendada por Davidson & MacKinnon para muestras pequeñas/moderadas: $e_i^2 / (1 - h_{ii})^2$.

```ghl
// recipe_02_robust_regression.gh
// Fit OLS and compute robust standard errors (HC0 - HC3)

// 1. Ingest cross-sectional economic data
let econ_data = dataframe {
    firm_size: [12.0, 18.0, 25.0, 40.0, 65.0, 90.0, 140.0, 210.0, 350.0, 500.0],
    rd_spend:  [1.1,  2.0,  2.4,  5.8,  6.2,  11.5, 12.0,  26.0,  30.5,  68.0],
    patent_count: [3,  4,   6,   10,   12,   19,   22,    41,    55,    98]
};

// 2. Fit standard OLS model
let model = ols(patent_count ~ firm_size + rd_spend, econ_data);

// 3. Extract classical variance-covariance matrix
let cov_classical = vcov(model, "Classical");

// 4. Compute robust HC3 variance-covariance matrix
let cov_hc3 = vcov(model, "HC3");

// 5. Compare standard errors for rd_spend:
// beta_idx: 0 -> Intercept, 1 -> firm_size, 2 -> rd_spend
let se_classical = sqrt(cov_classical[2, 2]);
let se_robust_hc3 = sqrt(cov_hc3[2, 2]);

// 6. Compute robust t-statistic and display
let beta_rd = coef(model)[2];
let robust_t_stat = beta_rd / se_robust_hc3;
```

---

## Receta 3: Clasificación Binaria con Regresión Logística (IRLS)

### Problema
Para modelar probabilidades de ocurrencia de un evento binario $y_i \in \{0, 1\}$ (ej. abandono de clientes o diagnóstico positivo), la regresión lineal puede predecir probabilidades fuera del intervalo $[0, 1]$. La regresión logística modela el log-odds mediante la función sigmoide:

$$p(y_i = 1 | x_i) = \frac{1}{1 + e^{-x_i^T \beta}}$$

### Solución en GHL
La función `fit_logistic()` ajusta el Modelo Lineal Generalizado (GLM) con distribución binomial y función de enlace logit mediante Mínimos Cuadrados Ponderados Iterativamente (**IRLS**), asegurando convergencia cuadrática rápida.

```ghl
// recipe_03_logistic_classification.gh
// Binary classification with IRLS Logistic Regression

// 1. Ingest customer subscription dataset
let churn_data = dataframe {
    tenure_months: [2.0, 5.0, 12.0, 18.0, 24.0, 36.0, 48.0, 60.0, 3.0, 8.0],
    monthly_bill:  [85.0, 95.0, 70.0, 65.0, 50.0, 45.0, 40.0, 35.0, 90.0, 80.0],
    support_calls: [5.0, 4.0, 2.0, 1.0, 1.0, 0.0, 0.0, 0.0, 4.0, 3.0],
    churned:       [1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0]
};

// 2. Fit binary logistic regression model via IRLS
let logit_model = fit_logistic(churned ~ tenure_months + monthly_bill + support_calls, churn_data);

// 3. Inspect model coefficients and deviance in Cockpit Deck
summary(logit_model);

// 4. Predict probabilities on new customer prospects
let prospects = dataframe {
    tenure_months: [4.0, 42.0],
    monthly_bill:  [88.0, 42.0],
    support_calls: [3.0, 0.0]
};

let predicted_probs = predict(logit_model, prospects);
// Classification threshold at 0.50
let classifications = predicted_probs |> map(|p| if p >= 0.5 { 1 } else { 0 });
```

---

## Receta 4: Agrupamiento No Supervisado con Modelos de Mezclas Gaussianas (GMM / EM)

### Problema
El agrupamiento simple con K-Means asume conglomerados esféricos de igual volumen y asignaciones duras (*hard clustering*). Los **Modelos de Mezclas Gaussianas (GMM)** permiten densidades elípticas y proporcionan asignaciones probabilísticas (*soft clustering*) a través del algoritmo de Esperanza-Maximización (**EM**).

$$\ln p(X | \pi, \mu, \Sigma) = \sum_{i=1}^N \ln \left( \sum_{k=1}^K \pi_k \mathcal{N}(x_i | \mu_k, \Sigma_k) \right)$$

### Solución en GHL
GHL proporciona la función `fit_gmm(data, k)` en la biblioteca estándar y soporte numérico para `log_sum_exp()` para evitar el subdesbordamiento aritmético (*underflow*) en el paso E.

```ghl
// recipe_04_gmm_clustering.gh
// Unsupervised clustering with Gaussian Mixture Models (EM)

// 1. Prepare bivariate spatial or financial observation matrix
let observations = mat [
    -2.9, -3.1 ;
    -3.2, -2.8 ;
    -2.7, -3.0 ;
     3.8,  4.1 ;
     4.2,  3.9 ;
     4.0,  4.3
];

// 2. Fit GMM with K = 2 mixture components
let gmm_fit = fit_gmm(observations, 2);

// 3. Render cluster parameters (means, mixture weights, covariances)
summary(gmm_fit);

// 4. Custom EM step implementation with log_sum_exp stability:
fn e_step(X, mu, pi, K) {
    let N = rows(X);
    let mut gamma = zeros(N, K);
    let mut i = 0;
    while i < N {
        let x_i = get_row(X, i);
        let mut log_p = zeros(K);
        let mut k = 0;
        while k < K {
            let diff = x_i - get_row(mu, k);
            let log_density = -0.5 * dot(diff, diff);
            log_p = set(log_p, k, log(get(pi, k)) + log_density);
            k = k + 1;
        };
        let lse = log_sum_exp(log_p);
        let mut k2 = 0;
        while k2 < K {
            gamma = set(gamma, i, k2, exp(get(log_p, k2) - lse));
            k2 = k2 + 1;
        };
        i = i + 1;
    };
    gamma
}
```

---

## Receta 5: Muestreo Bayesiano MCMC y Bootstrap Paralelo Reproducible

### Problema
1. La inferencia bayesiana para modelos jerárquicos requiere generar muestras de la distribución a posteriori mediante cadenas de Markov Monte Carlo (MCMC).
2. El remuestreo Bootstrap requiere recalcular estadísticos en miles de submuestras, lo cual puede resultar costoso si se ejecuta de forma secuencial en un solo hilo.

### Solución en GHL
GHL permite implementar muestreadores de Gibbs vectorizados con generadores de números pseudoaleatorios reproducibles (`random_normal`, `random_gamma`) y paralelizar remuestreos instantáneamente con `.par_iter()`.

```ghl
// recipe_05_mcmc_and_bootstrap.gh
// Bayesian Gibbs Sampler and Multi-threaded Parallel Bootstrap

// ==========================================
// Part A: Bayesian Gibbs Sampler for Normal Means
// ==========================================
fn run_gibbs_sampler(y, groups, num_iterations) {
    let num_groups = max(groups) + 1;
    let mut trace = zeros(num_iterations, num_groups);
    let mut mu_vec = zeros(num_groups);
    let tau = 1.0; // Precision hyperparameter

    let mut iter = 0;
    while iter < num_iterations {
        let mut g = 0;
        while g < num_groups {
            let y_g = filter(y, groups == g);
            let n_g = len(y_g);
            let post_mean = (sum(y_g) * tau) / (n_g * tau + 1.0);
            let post_sd = 1.0 / sqrt(n_g * tau + 1.0);
            let draw = first(random_normal(1, post_mean, post_sd));
            mu_vec = set(mu_vec, g, draw);
            g = g + 1;
        };
        trace = set_row(trace, iter, mu_vec);
        iter = iter + 1;
    };
    trace
}

// ==========================================
// Part B: Parallel Bootstrap Resampling
// ==========================================
// Estimate 95% Confidence Interval for the sample median across 10,000 resamples
let original_sample = [14.2, 16.8, 19.5, 21.0, 22.4, 25.1, 28.3, 31.0, 35.2, 42.0];
let n_obs = len(original_sample);
let num_bootstraps = 5_000;

// Zero-overhead parallel iterator distributing iterations across all CPU cores
let bootstrap_medians = (0..num_bootstraps)
    .par_iter()
    .map(|seed_idx| {
        // Sample with replacement
        let resampled = sample_n(original_sample, n_obs, replace: true);
        median(resampled)
    })
    .collect();

let se_boot = std_dev(bootstrap_medians);
let lower_ci = quantile(bootstrap_medians, 0.025);
let upper_ci = quantile(bootstrap_medians, 0.975);
```
