# RFC 09: Datos Categóricos (Factores), Gramática de Gráficos y Validación Numérica (NIST StRD)

- **Estado:** Aprobado
- **Área:** Sistema de Tipos / Visualización / Precisión Numérica / Calidad
- **Audiencia:** Diseñadores del compilador, estadísticos, desarrolladores de la biblioteca estándar

---

## 1. Motivación y Visión

Este RFC define tres pilares fundamentales que posicionan a **GHL** como un entorno de computación estadística superior:
1. **Tratamiento nativo de variables categóricas (`Factor` y `OrderedFactor`)** superando las deficiencias históricas de R, Python y Julia.
2. **Gramática de Gráficos declarativa integrada (`std::plot`)** sobre el motor `plotters`, con salida a archivos (PNG/SVG) y terminal interactiva (Unicode).
3. **Certificación de precisión numérica contra los datasets NIST StRD**, garantizando exactitud matemática verificable de hasta 15 decimales.

---

## 2. Variables Categóricas y Factores de Primer Nivel (`Factor` / `OrderedFactor`)

### 2.1. Análisis Crítico: ¿Qué evitar de otros lenguajes?
- **De R:** Evitar la coerción implícita destructiva (`stringsAsFactors` que arruinó análisis durante dos décadas), la caída silenciosa de niveles al filtrar datos, y la representación basada en enteros 1-based desvinculada de estándares de memoria modernos.
- **De Python (Pandas):** Evitar la fragmentación de la API (accesor `.cat`, comportamientos inconsistentes al hacer merge/join entre categorías con distintos niveles, y la falta de integración nativa con matrices de modelos lineales).
- **De Julia (`CategoricalArrays.jl`):** Evitar que las categorías sean una librería externa pesada con penalizaciones de tipado dinámico y problemas de invalidación de métodos.

### 2.2. Diseño de Factores en GHL

GHL introduce `Factor<T>` (categórico nominal) y `OrderedFactor<T>` (categórico ordinal) como tipos nativos de primera clase:

```lang
// Creation with explicit or inferred levels:
let treatment = factor(["control", "dose_a", "dose_b", "control"]);

// Ordered factor with strict mathematical transitivity:
let education = ordered_factor(
    ["bachelor", "phd", "master", "high_school"],
    levels: ["high_school", "bachelor", "master", "phd"]
);

education[1] > education[0]; // true: phd > bachelor
```

### 2.3. Estructura de Memoria (Arrow Dictionary Encoding)
- Los factores se almacenan como un **índice entero compacto** sin signo (`u8` para hasta 256 niveles, `u16` para hasta 65.536 niveles) que apunta a un array inmutable de valores únicos (búfer de niveles).
- **Eficiencia:** Un DataFrame con 10 millones de filas de texto repetido ocupa una fracción mínima de RAM y se compara en registros SIMD como simples enteros `u8`.

### 2.4. Integración con Modelos Lineales y Matrices de Contraste
Cuando un `Factor` se utiliza en una fórmula de regresión (`y ~ treatment + age`), GHL lo expande automáticamente según una matriz de contrastes configurable:

```lang
// Default: Treatment / Dummy coding (first level is reference)
let fit1 = linear_model(response ~ treatment, data: &df)?;

// Explicit contrast specification directly in formula or column:
let fit2 = linear_model(
    response ~ contrast(treatment, Contrast::Sum), // Sum / Deviation coding (effects sum to 0)
    data: &df
)?;
```

Contrastes soportados nativamente:
- `Contrast::Treatment(ref_level)`: Dummy coding estándar (referencia contra cada nivel).
- `Contrast::Sum`: Codificación por desviación (la media de los coeficientes es cero).
- `Contrast::Helmert`: Cada nivel se compara con la media de los niveles anteriores.
- `Contrast::Polynomial`: Para factores ordenados (tendencia lineal, cuadrática, cúbica).

---

## 3. Gramática de Gráficos Declarativa (`std::plot`)

Para evitar que los usuarios dependan de herramientas externas para visualizar datos, GHL incorpora una gramática declarativa basada en la teoría de Leland Wilkinson (`ggplot2`), impulsada por el motor **`plotters`**.

### 3.1. Sintaxis con Operador de Tubería (`|>`)
A diferencia de R (donde `ggplot2` requiere usar `+` y falla si el operador queda en la línea siguiente), GHL usa el operador de tubería nativo `|>`:

```lang
use std::plot::{plot, aes, geom_point, geom_smooth, geom_histogram, labs, theme_minimal};

df  |> filter(col("score") > 0.0)
    |> plot(aes(x: col("dose"), y: col("response"), color: col("treatment")))
    |> geom_point(alpha: 0.8, size: 3.0)
    |> geom_smooth(method: LM, se: true)
    |> labs(
        title: "(=^･ω･^=) Dose-Response Relationship",
        x: "Administered Dose (mg)",
        y: "Observed Response"
    )
    |> theme_minimal()
    |> save("output/dose_response.png")?;
```

### 3.2. Visualización Interactiva en Terminal (Modo REPL)
En el REPL interactivo de GHL, encadenar `|> show()` dibuja automáticamente el gráfico dentro de la terminal usando caracteres Unicode / Braille de alta resolución:

```
     Dose-Response Scatter (Terminal Preview)
  40 ┤                                     * 
  30 ┤                        *    * *       
  20 ┤             *   *  *                  
  10 ┤   *   *                               
   0 ┼───┬─────────┬─────────┬─────────┬─────
     0   2         4         6         8 (Dose)
  (U・ᴥ・U) Haru rendered 1,000 points in 1.8ms!
```

---

## 4. Validación Numérica Oficial: Benchmarks NIST StRD

Para certificar la exactitud matemática de GHL frente a la comunidad científica internacional, el compilador y la biblioteca estándar integran la suite completa de pruebas del **NIST** (*National Institute of Standards and Technology* - *Statistical Reference Datasets*).

### 4.1. Conjuntos de Datos Evaluados
1. **Regresión Lineal con Matriz Mal Condicionada:**
   - **Dataset de Longley:** Prueba extrema de colinealidad multivariada (coeficiente de correlación > 0.99 entre regresores).
   - **Dataset de Filip:** Polinomio de grado 10 con matriz de Vandermonde severamente mal condicionada.
   - **Dataset de Wampler:** Evaluación de precisión en presencia de números de orden de magnitud dispar.
2. **Regresión No Lineal y Optimización:**
   - **Misra1a, Bennett5, BoxBOD:** Ajuste no lineal con mínimos cuadrados amortiguados (Levenberg-Marquardt).
3. **Estadísticas Univariadas:**
   - Media, desviación estándar y autocorrelación en series con números gigantes y varianzas casi nulas (pruebas de cancelación catastrófica).

### 4.2. Tolerancia y Criterio de Certificación en CI/CD
Toda versión de GHL (`ghl test --nist`) debe coincidir con las soluciones certificadas por el NIST:
$$\text{LRE} = -\log_{10} \left( \frac{|\hat{\beta}_{\text{GHL}} - \beta_{\text{NIST}}|}{|\beta_{\text{NIST}}|} \right) \ge 12$$
GHL exige un mínimo de **12 a 15 dígitos de precisión relativa (LRE)** en todos los benchmarks del NIST antes de aprobar cualquier optimización en el compilador o en `faer`.

### 4.3. Detección Preventiva de Mal Condicionamiento
Si una matriz de entrada en código de usuario tiene un número de condición $\kappa(A) > 10^{10}$, GHL no entrega números basura en silencio; emite un diagnóstico proactivo:
```
(・`ω´・) [Statistical Warning SW030]: Ill-Conditioned Design Matrix (κ = 4.2e12)
  ┌─ analysis.gh:14:12
  │
14│     let fit = linear_model(y ~ x1 + x2 + x3, data: &df)?;
  │               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  │
  = Note: Matrix condition number is near the limit of float64 precision.
  = Help: GHL automatically employed QR decomposition with column pivoting.
          Consider standardizing predictors: `df |> scale(["x1", "x2", "x3"])`.
```

