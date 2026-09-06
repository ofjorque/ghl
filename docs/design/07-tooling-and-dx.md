# RFC 07: Experiencia de Desarrollo (DX), Diagnósticos con Kaomojis y Herramientas en GHL

- **Estado:** Propuesto
- **Área:** Tooling / Diagnósticos del Compilador / REPL / LSP / UX
- **Audiencia:** Diseñadores de herramientas, autores del compilador, usuarios de GHL

---

## 1. Filosofía de DX: Rigor Matemático con Calidez Empática

La mayoría de los compiladores de sistemas (C++, Rust) tratan los errores con frialdad severa, mientras que los entornos dinámicos (R, Python) con frecuencia emiten *tracebacks* crípticos o advertencias estadísticas silenciosas.

**GHL adopta una filosofía centrada en las personas y amantes de las mascotas:**
- Los errores son oportunidades de aprendizaje guiado, no castigos.
- **Kaomojis expresivos** proporcionan un tono relajado, empático y lúdico que reduce la frustración del investigador.
- **Rigor absoluto:** La calidez visual no disminuye la precisión formal: cada diagnóstico incluye la explicación matemática o informática exacta, la ubicación de código y sugerencias accionables.

---

## 2. Taxonomía Dual de Diagnósticos del Compilador

GHL divide formalmente todos sus diagnósticos en dos mundos claramente delimitados:

```
                            SISTEMA DE DIAGNÓSTICOS GHL
                                         │
                 ┌───────────────────────┴───────────────────────┐
                 ▼                                               ▼
     [ COMPUTE ERRORS (Cxxxx) ]                     [ STATISTICAL ERRORS (Sxxxx) ]
  Problemas de Ciencias de la Computación        Problemas de Estadística y Modelado
  • Desajustes de tipo en memoria                • Matrices singulares / no invertibles
  • Errores de sintaxis / gramática              • Falta de identificación en modelos
  • Índices fuera de límites                     • Covarianzas no definidas positivas
  • Errores de asignación y búfer                • Divergencias en cadenas MCMC
  • Fallos de I/O, red y permisos                • Violaciones de soporte en distribuciones
```

---

## 3. Catálogo de Kaomojis y Ejemplos Visuales

### 3.1. Errores Computacionales (`Compute Error [Cxxxx]`)
Utilizan Kaomojis que reflejan tropiezos o desconcierto informático: `(ノ°□°)ノ`, `(╯°□°)╯︵ ┻━┻`, `(｡•́︿•̀｡)`

```
(ノ°□°)ノ [Compute Error C0102]: Type Mismatch in Assignment
  ┌─ src/simulation.gh:34:15
  │
34│     let count: i64 = 42.85;
  │                      ^^^^^ Expected 64-bit integer (`i64`), found 64-bit float (`f64`)
  │
  = Note: GHL does not perform implicit lossy truncation from float to integer.
  = Help: Explicitly cast using `.to_i64()` or `.round()`:
          `let count: i64 = 42.85.round();`
```

```
(╯°□°)╯︵ ┻━┻ [Compute Error C0215]: Vector Index Out of Bounds
  ┌─ src/etl.gh:12:9
  │
12│     let val = data[10];
  │               ^^^^^^^^ Attempted to access index 10, but vector length is 10 (valid indices: 0..9)
  │
  = Help: GHL uses 0-based indexing. The last valid element is `data[9]`.
```

---

### 3.2. Errores Estadísticos (`Statistical Error [Sxxxx]`)
Utilizan Kaomojis con la mirada analítica y vigilante de los felinos: `ฅ(ﾐΦ ﻌ Φﾐ)ฅ`, `(・`ω´・)`, `(=^･ω･^=)`

```
ฅ(ﾐΦ ﻌ Φﾐ)ฅ [Statistical Error S0301]: Singular Covariance Matrix Detected
  ┌─ src/glm.gh:88:18
  │
88│     let beta = inv(X.t() * X) * X.t() * y;
  │                ^^^^^^^^^^^^^^ Matrix is rank-deficient (rank = 3, columns = 5)
  │
  = Mathematical Diagnosis: Perfect multicollinearity detected among regressors.
    Columns 2 (`age_months`) and 4 (`age_years`) are linearly dependent.
  = Statistical Remedy: Drop redundant predictors or apply Ridge regularization:
    `ridge_regression(X, y, lambda: 1e-4)`
```

```
(・`ω´・) [Statistical Error S0412]: Distribution Support Violation
  ┌─ src/inference.gh:51:24
  │
51│     let p = Poisson::new(lambda: -2.5)?;
  │                                  ^^^^ Parameter `lambda` must be strictly positive (λ > 0)
  │
  = Mathematical Diagnosis: The Poisson distribution rate parameter λ represents an expected
    count and is only defined on the positive real numbers (0, +∞).
```

---

### 3.3. Advertencias Estadísticas (`Statistical Warning [SWxxxx]`)
Gojo interviene cuando la computación es válida, pero las conclusiones numéricas o estadísticas corren peligro: `(ФωФ)`

```
(ФωФ) [Statistical Warning SW008]: High Multicollinearity Detected
  ┌─ src/regress.gh:22:5
  │
22│     linear_model(response ~ x1 + x2 + x3, data: &df);
  │     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  │
  = Warning: Variance Inflation Factor (VIF) for predictor `x2` is 14.8 (> 10.0).
  = Impact: Standard errors of coefficients may be severely inflated.
```

```
(ФωФ) [Statistical Warning SW021]: Small Sample Size for Asymptotic Inference
  ┌─ src/tests.gh:15:9
  │
15│     t_test(&sample_a, &sample_b);
  │     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Sample size is N = 4 (< 5)
  │
  = Warning: Asymptotic normality cannot be safely assumed with N < 5.
  = Suggestion: Consider a non-parametric permutation test: `permutation_test(&a, &b)`.
```

---

### 3.4. Mensajes de Éxito y Consejos Caninos (Haru)
Cuando los modelos convergen o las operaciones terminan con éxito, Haru celebra el logro: `(U・ᴥ・U)`

```
(U・ᴥ・U) Haru fetched the results!
  • MCMC Chain: 4 chains × 10,000 draws completed in 1.42s
  • Convergence Check: All parameters reached R-hat < 1.01 (No divergent transitions)
  • Effective Sample Size (ESS): Mean ESS = 8,420 draws
```

---

## 4. REPL Interactivo y Formateo Visual de Datos

El REPL de GHL (`ghl repl`) arranca en menos de **20 ms** y ofrece:
- **Bienvenida personalizada:**
  ```
     /\_/\     GHL Interactive Shell (v0.1.0)
    ( o.o )    Type :help for commands, :quit to exit.
     > ^ <     Ready to test your hypotheses! (=^･ω･^=)
  ```
- **Formateo inteligente de DataFrames:**
  Impresión automática de dimensiones, tipos de columnas y conteo de valores faltantes `NA`.
- **Comandos del REPL:**
  - `:time <expr>`: Perfila tiempo y memoria sin pausas.
  - `:type <expr>`: Muestra el tipo inferido estáticamente.
  - `:disasm <expr>`: Muestra el código ensamblador generado.

---

## 5. Servidor de Lenguaje Oficial (LSP) y Formateador

- **LSP Integrado (`ghl lsp`):** Diagnósticos inmediatos en VS Code, Neovim y Zed con soporte de Kaomojis y sugerencias rápidas.
- **Formateador Oficial (`ghl fmt`):** Mantiene un estándar consistente en todo el código del ecosistema.
- **Linter Estadístico (`ghl check`):** Alerta proactivamente sobre cancelaciones numéricas catastróficas y divisiones por cero en matrices.

