# RFC 02: Sistema de Tipos y Semántica de GHL

- **Estado:** Propuesto
- **Área:** Sistema de Tipos / Semántica Formal
- **Audiencia:** Diseñadores del lenguaje, implementadores del verificador de tipos (Type Checker)

---

## 1. Fundamentos del Sistema de Tipos de GHL

GHL utiliza un **sistema de tipos estático, fuerte y seguro** con **inferencia de tipos bidireccional local** (Hindley-Milner extendido). Ofrece la agilidad y fluidez de Python/R en scripts interactivos, sin renunciar a la seguridad en tiempo de compilación y optimizaciones de bajo nivel de Rust:

```lang
// Local type inference frees the user from redundant annotations:
let observations = [1.2, 3.4, 5.6];        // Inferred as Vector<f64, 3>
let sample_mean = observations.mean();      // Inferred as f64
let is_valid = sample_mean > 2.0;           // Inferred as bool
```

---

## 2. Tratamiento Formal de Datos Faltantes (`NA`)

A diferencia de Python (donde conviven caóticamente `None`, `np.nan` y `pd.NA`) o C++ (que carece de semántica para datos ausentes), GHL establece una semántica unificada:

### 2.1. Distinción Rigurosa entre `NA` y `NaN`
- **`NaN` (Not a Number):** Resultado de una operación aritmética con coma flotante indefinida (ej. `0.0 / 0.0` o `sqrt(-1.0)`). Sigue estrictamente la especificación IEEE-754.
- **`NA` (Not Available):** Representa formalmente un dato no observado o faltante en una muestra estadística.

### 2.2. NAs Universales con Motivo Opcional (`NA` y `NA:Reason`)

En GHL, **todo dato faltante es fundamentalmente `NA`**. La gran mayoría de las veces el programador simplemente escribe `NA` sin preocuparse por nada más:

```lang
// Natural, frictionless default: NA without any reason
let data = [1.0, 2.0, NA, 4.0, 5.0];
```

Sin embargo, para casos donde la causa del dato ausente sea relevante para el análisis estadístico posterior (ej. encuestas, sensores, ensayos clínicos), GHL permite que **cualquiera pueda adjuntarle un motivo opcionalmente**:

```lang
// Optional semantic annotations: All of these are still valid NAs!
let survey_answer = NA:NoResponse;       // Optional reason: subject skipped question
let unobserved    = NA:NotObservable;     // Optional reason: phenomenon cannot be observed
let pregnancy_q   = NA:NotApplicable;     // Optional reason: question not applicable to cohort
let lab_reading   = NA:SensorDropout;     // Optional reason: device lost signal
let default_na    = NA;                   // Simple NA (no reason attached)
```

#### Invariantes de Diseño (Cero Fricción)
1. **Unificación Total:** Todo `NA:Reason` **es** un `NA`. Las funciones de la biblioteca estándar (`mean`, `sum`, `filter`, `linear_model`) los tratan como `NA` sin requerir adaptaciones:
   ```lang
   let x = NA:NotObservable;
   x.is_na(); // true
   ```
2. **Introspección Opcional:** Si un `NA` no tiene motivo, `.na_reason()` simplemente devuelve `None`. Solo devuelve `Some(...)` si fue anotado explícitamente:
   ```lang
   let plain_na = NA;
   plain_na.na_reason(); // None

   let noted_na = NA:NotObservable;
   noted_na.na_reason(); // Some(NAReason::NotObservable)
   ```
3. **Comportamiento Aritmético:** Toda operación matemática (`val + NA:NotObservable`) evalúa a `NA`. Si se requiere, preserva el motivo disponible de forma transparente.
4. **Almacenamiento Eficiente:** Si una columna solo usa `NA` plano, ocupa únicamente el bitmask estándar de Apache Arrow (0 bytes de sobrecarga). Solo si se usan motivos se activa el vector categórico opcional en segundo plano.

   // Filtering or imputing by reason:
   let clean_df = df
       .filter_na_reason(col("salary"), drop: [NAReason::NoResponse])
       .impute(col("measurement"), strategy: Mean, only_for: [NAReason::SensorDropout]);
   ```

### 2.3. Lógica Ternaria de Kleene
Las expresiones lógicas que involucran cualquier variante de `NA` se evalúan bajo la lógica de tres estados de Kleene:

| Expresión | Valor Evaluado | Justificación Formal |
| :--- | :--- | :--- |
| `true && NA:NoResponse` | `NA:NoResponse` | El resultado final depende del valor ausente. |
| `false && NA:Reason`    | `false`         | Un operando `false` determina la conjunción independientemente de `NA`. |
| `true || NA:Reason`     | `true`          | Un operando `true` determina la disyunción independientemente de `NA`. |
| `false || NA:Reason`    | `NA:Reason`     | El resultado final depende del valor ausente. |
| `!NA:Reason`            | `NA:Reason`     | La negación de un estado desconocido preserva su causa. |
| `10.0 == NA:Reason`     | `NA:Reason`     | No es posible determinar igualdad contra un valor no observado. |

### 2.4. Propagación y Agregación
Las funciones estadísticas no silencian la presencia de datos faltantes por omisión:
```lang
let data = [10.0, 20.0, NA:NoResponse];

mean(data);               // Returns: NA:NoResponse
mean(data, skip_na: true) // Returns: 15.0
```

### 2.5. Representación en Memoria Columnar (Arrow Native con Zero-Overhead)

> **Nota de implementación (2026-09-07):** una versión anterior de esta sección
> especificaba un diccionario columnar `u8` para los motivos. Se descartó a propósito:
> el objetivo de los motivos de NA es que otros paquetes de análisis de datos perdidos
> puedan consumirlos, y para eso lo que importa es que sean **livianos, simples y
> fáciles de exponer** — no la codificación de memoria más compacta posible. Un
> diccionario `u8` agrega encoding/decoding y sincronización sin necesidad real. El
> diseño de abajo es el que implementa `crates/ghl-runtime/src/na_reasons.rs`.

Para no penalizar el rendimiento ni la compatibilidad con Apache Arrow:
1. **Representación Estándar (Sin Motivo):** Cuando una columna solo contiene datos observados y `NA` genéricos, se utiliza únicamente el **bitmask de validez de 1 bit de Apache Arrow** (coste cero adicional) — GHL nunca aparta memoria para motivos si ninguna celda los tiene.
2. **Representación Semántica (Con Motivos):** Si el DataFrame incluye motivos (`NA:Reason`), GHL mantiene una tabla lateral liviana de strings UTF-8 planos, indexada por `(columna, fila)` — sin diccionario, sin codificación, un `String` por motivo registrado. Se reindexa automáticamente junto con cualquier verbo que reordene o filtre filas (`filter`, `arrange`, `slice`, `sample_n`, `distinct`) y se descarta explícitamente en verbos que colapsan filas (`group_by`/`summarize`, donde "qué motivo gana" no está definido) o que no tienen una fila de origen clara (`inner_join`/`left_join`).
3. **Exposición al lenguaje:** los motivos no son solo un detalle interno — existen justamente para que herramientas de análisis de datos perdidos (propias o de terceros) puedan consultarlos como datos GHL comunes:
   - `na_reason(x)` — motivo de un valor individual (`Some(reason)` → `Value::String`, `None` → `NA`), coincide con `x.na_reason()` de la sección 2.2.
   - `na_reasons(df, col)` — un `Vector` del mismo largo que la columna, con el motivo en cada fila que lo tiene y `NA` en el resto; se puede usar con cualquier verbo existente (`filter`, `count`, `group_by`) sin una API de consulta nueva.

---

## 3. Tipos Dimensionales y Comprobación de Formas (Shapes)

Para evitar desajustes de dimensiones en álgebra matricial y modelos tensoriales:

```lang
// Compile-time verified matrix multiplication via const generics:
fn matmul<const ROWS_A: usize, const INNER: usize, const COLS_B: usize>(
    a: &Matrix<f64, ROWS_A, INNER>,
    b: &Matrix<f64, INNER, COLS_B>
) -> Matrix<f64, ROWS_A, COLS_B> {
    // If dimensions do not align, GHL rejects the code at compile time
}
```

Para datos leídos en tiempo de ejecución (ej. un CSV con $N$ filas desconocidas):
- GHL soporta dimensiones dinámicas (`Matrix<f64, Dynamic, Dynamic>`).
- Las verificaciones en tiempo de ejecución se optimizan mediante elisión de límites (*bounds-check elimination*).

---

## 4. Polimorfismo: Traits con Monomorfización vs. Múltiple Despacho

GHL selecciona **Traits estáticos con monomorfización AOT** en lugar de múltiple despacho dinámico (como Julia).
- **Razón:** Garantiza compilación Ahead-Of-Time predecible, elimina la invalidación de métodos en cadena (*method invalidations*), evita ambigüedades silenciosas y permite generar ejecutables nativos pequeños.

```lang
trait Distribution {
    type Output;
    fn sample(&self, rng: &mut RNG) -> Self::Output;
    fn log_pdf(&self, x: Self::Output) -> f64;
    fn cdf(&self, x: Self::Output) -> f64;
}

struct NormalDistribution {
    mean: f64,
    std_dev: f64,
}

impl Distribution for NormalDistribution {
    type Output = f64;

    fn sample(&self, rng: &mut RNG) -> f64 {
        // Box-Muller or Ziggurat algorithm implementation
    }

    fn log_pdf(&self, x: f64) -> f64 {
        let diff = (x - self.mean) / self.std_dev;
        -0.5 * diff * diff - ln(self.std_dev * sqrt(2.0 * PI))
    }

    fn cdf(&self, x: f64) -> f64 {
        0.5 * (1.0 + erf((x - self.mean) / (self.std_dev * sqrt(2.0))))
    }
}
```

---

## 5. Tipos Algebraicos y Manejo de Errores Tipados

GHL no utiliza excepciones opacas que detienen el programa inesperadamente. Los errores son valores tipados diferenciados formalmente:

```lang
enum Result<T, E> {
    Ok(T),
    Err(E),
}

// Differentiation between computation and statistical anomalies:
enum AnalysisError {
    Compute(ComputeError),
    Statistical(StatisticalError),
}
```

