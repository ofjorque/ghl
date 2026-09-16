# Guía de Migración para Usuarios de R a GHL

Bienvenido a **GHL** (*Generalized Hypothesis Language*, conocido como *Gojo & Haru Language*). Si vienes del ecosistema de **R** y el **Tidyverse**, encontrarás en GHL un entorno diseñado específicamente para hacerte sentir como en casa, pero con la velocidad, seguridad de tipos y ausencia de pausas de recolección de basura (*Tracing Garbage Collector*) de un lenguaje de sistemas compilado con Cranelift/LLVM.

Esta guía detalla las correspondencias exactas entre la sintaxis idiomática de R/Tidyverse y la semántica de primer orden de GHL.

---

## 1. Tabla de Equivalencias de Funciones Estadísticas

En R, el modelado lineal clásico depende de `lm()`, `glm()` y métodos genéricos S3. En GHL, el framework de modelado estadístico **NEKO** (RFC 11) desacopla la especificación, el ajuste numérico y la inferencia asintótica, proporcionando tipos estructurados y diagnósticos enriquecidos mediante **Cockpit Deck** (RFC 12).

| Tarea Estadística | R (Base / Stats) | GHL (Standard Library) |
| :--- | :--- | :--- |
| **Regresión Lineal Simple / Múltiple** | `fit <- lm(mpg ~ wt + hp, data = mtcars)` | `let model = ols(mpg ~ wt + hp, mtcars);` *(o alias `fit`)* |
| **Regresión Logística Binaria (GLM)** | `fit <- glm(churn ~ age + tenure, data = df, family = binomial)` | `let model = fit_logistic(churn ~ age + tenure, df);` |
| **Resumen e Inferencia del Modelo** | `summary(fit)` | `summary(model);` |
| **Predicciones sobre Nuevos Datos** | `predict(fit, newdata = test_df)` | `let preds = predict(model, test_df);` |
| **Extracción de Residuos** | `residuals(fit)` | `let res = residuals(model);` |
| **Coeficientes Estimados ($\hat{\beta}$)** | `coef(fit)` | `let betas = coef(model);` |
| **Matriz de Covarianza Clásica** | `vcov(fit)` | `let cov = vcov(model, "Classical");` |
| **Errores Estándar Robustos (Sandwich)** | `sandwich::vcovHC(fit, type = "HC3")` | `let cov_hc3 = vcov(model, "HC3");` *(HC0 a HC3 nativos)* |
| **Agrupamiento Gaussiano (EM / GMM)** | `mclust::Mclust(data, G = 3)` | `let gmm = fit_gmm(data, 3);` |
| **Medidas Descriptivas** | `mean(x)`, `sd(x)`, `median(x)`, `var(x)` | `mean(x)`, `std_dev(x)`, `median(x)`, `var(x)` |

### Ejemplo Comparativo: Regresión Lineal OLS

#### En R:
```r
library(datasets)

# Fit linear model
fit <- lm(mpg ~ wt + hp, data = mtcars)

# Print summary
summary(fit)

# Extract predictions
preds <- predict(fit, newdata = mtcars)
```

#### En GHL:
```ghl
// Load dataset and fit OLS model with Wilkinson-Rogers formula
let model = ols(mpg ~ wt + hp, mtcars);

// Render Cockpit diagnostics panel
summary(model);

// Extract predictions vector
let preds = predict(model, mtcars);
let betas = coef(model);
```

---

## 2. Comparativa Tidyverse vs. GHL DataFrames

El operador pipe de R (`%>%` de magrittr o `|>` de R base) es un ciudadano de primera clase en GHL: `|>`. 

En GHL, las funciones de transformación de DataFrames (`filter`, `mutate`, `select`, `group_by`, `summarize`, `arrange`, `inner_join`, `left_join`) operan en **contexto columnar** (*column context*): las columnas no requieren comillas dentro de los verbos y se evalúan directamente sin overhead interpretativo.

### Tabla de Verbos Principales

| Operación | R (dplyr / Tidyverse) | GHL (Sintaxis Canónica) |
| :--- | :--- | :--- |
| **Filtrado de Filas** | `df %>% filter(age >= 18 & status == "active")` | `df \|> filter(age >= 18 && status == "active")` |
| **Creación/Mutación de Columnas** | `df %>% mutate(ratio = hp / wt, log_mpg = log(mpg))` | `df \|> mutate(ratio = hp / wt, log_mpg = log(mpg))` |
| **Selección de Columnas** | `df %>% select(mpg, wt, hp)` | `df \|> select(mpg, wt, hp)` |
| **Ordenamiento** | `df %>% arrange(desc(mpg), wt)` | `df \|> arrange(desc(mpg), wt)` |
| **Agrupamiento y Resumen** | `df %>% group_by(cyl) %>% summarize(m = mean(mpg))` | `df \|> group_by(cyl) \|> summarize(m = mean(mpg))` |
| **Conteo de Frecuencias** | `df %>% count(cyl)` | `df \|> count(cyl)` |
| **Uniones Relacionales (Joins)** | `inner_join(df1, df2, by = "id")` | `inner_join(df1, df2, by = "id")` *(o canalizado `df1 \|> inner_join(df2, "id")`)* |
| **Top/Bottom Filas** | `df %>% slice_max(mpg, n = 5)` | `df \|> slice_max(mpg, n = 5)` |

### Ejemplo Comparativo de Pipeline

#### En R (dplyr):
```r
library(dplyr)

result <- mtcars %>%
  filter(cyl > 4) %>%
  mutate(power_weight = hp / wt) %>%
  group_by(gear) %>%
  summarize(
    mean_power = mean(power_weight, na.rm = TRUE),
    total_cars = n()
  ) %>%
  arrange(desc(mean_power))
```

#### En GHL:
```ghl
// Idiomatic GHL pipeline: zero-cost columnar transformations
let result = mtcars
    |> filter(cyl > 4)
    |> mutate(power_weight = hp / wt)
    |> group_by(gear)
    |> summarize(
        mean_power = mean(power_weight),
        total_cars = count()
    )
    |> arrange(desc(mean_power));
```

---

## 3. Manejo de Valores Faltantes: De `NA` a `NA:Reason` y Kleene 3VL

Una de las innovaciones más importantes de GHL frente a R es la semántica de datos ausentes (RFC 02):

1. **R:** Posee un único valor centinela `NA` por tipo (`NA_real_`, `NA_integer_`, etc.). Si un dato falta, el motivo se pierde para siempre en el pipeline salvo que se gestione en columnas auxiliares manuales.
2. **GHL:** Soporta el `NA` clásico, pero introduce **motivos semánticos opcionales** mediante la sintaxis `NA:Reason`:
   - `NA:NoResponse`: El encuestado omitió la pregunta.
   - `NA:SensorDropout`: El sensor físico sufrió una desconexión momentánea.
   - `NA:MeasurementFailure`: Fallo de hardware o lectura ilegible.
   - `NA:NotObservable`: La variable no aplica físicamente al sujeto (ej. semanas de gestación en pacientes masculinos).
   - `NA:Censored`: Observación censurada a la derecha o izquierda en análisis de supervivencia.

### Lógica Trivaluada de Kleene (3VL)

Al igual que en R, las comparaciones lógicas respetan la lógica de Kleene, pero con preservación del motivo del fallo:
- `TRUE || NA` $\to$ `TRUE` (determinable sin importar el valor ausente).
- `FALSE && NA` $\to$ `FALSE`.
- `TRUE && NA` $\to$ `NA`.
- `NA == NA` $\to$ `NA` (en GHL dos incógnitas no son idénticas por definición).

### Limpieza e Imputación Condicional por Motivo

En R se suele usar `tidyr::replace_na()` o `ifelse(is.na(x), mean(x, na.rm=TRUE), x)`. En GHL existen primitivas estadísticas nativas conscientes del motivo:

```ghl
// Create DataFrame with rich semantic NAs
let df = dataframe {
    patient_id: [101, 102, 103, 104],
    salary: [50000.0, NA:NoResponse, 65000.0, 70000.0],
    measurement: [12.0, 14.0, NA:SensorDropout, 16.0]
};

// 1. Drop rows with non-ignorable missingness (NoResponse)
// 2. Impute only technical sensor dropouts using the sample mean
let clean_df = df
    |> filter_na_reason(salary, drop: [NAReason::NoResponse])
    |> impute(measurement, strategy: Mean, only_for: [NAReason::SensorDropout]);
```

---

## 4. Diferencias Clave de Diseño que Debes Conocer

1. **Indexación:** GHL utiliza indexación **0-based** (comienza en `0`), a diferencia de R que utiliza `1-based`. El primer elemento de `v` es `v[0]`.
2. **Sistema de Tipos e Inferencia:** GHL tiene tipado estático fuerte con inferencia local (estilo Rust/Swift). Las variables son inmutables por defecto (`let x = 1.0;`), requiriendo `let mut x = 1.0;` para reasignaciones.
3. **Puntos y Guiones Bajos:** En R es común usar puntos en nombres de variables (`my.data.frame`). En GHL los puntos denotan acceso a campos o llamadas a métodos; las variables usan `snake_case` (`my_data_frame`).
4. **Sin Overhead de Memoria:** A diferencia de las copias profundas inadvertidas de R, GHL utiliza Copy-on-Write (CoW) con conteo de referencias atómicas (ARC) optimizado. La mutación es *in-place* garantizada cuando el contador de referencias es 1.
