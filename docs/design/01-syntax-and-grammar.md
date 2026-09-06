# RFC 01: Sintaxis, Gramática y Ergonomía de GHL

- **Estado:** Propuesto
- **Área:** Especificación del Lenguaje / Frontend
- **Audiencia:** Diseñadores del lenguaje, autores de gramáticas, desarrolladores de herramientas

---

## 1. Objetivos de Diseño Sintáctico

La sintaxis de **GHL** satisface simultáneamente dos requisitos fundamentales:
1. **Claridad para matemáticos y estadísticos:** Código conciso y expresivo para análisis exploratorio, transformaciones de datos y especificación de modelos estadísticos.
2. **Rigor para desarrollo de sistemas y backend:** Gramática inequívoca (LALR(1) / PEG amigable), sin ambigüedades contextuales, facilitando herramientas de análisis estático rápido y formateo determinista.

El código fuente se escribe íntegramente en **inglés** (palabras clave, nombres de funciones en la biblioteca estándar y mensajes). El lenguaje está **orientado a expresiones**: bloques `{ ... }`, sentencias condicionales `if` y coincidencia de patrones `match` producen valores directamente.

---

## 2. Elementos Léxicos y Convenciones

- **Bloques y Delimitadores:** Se utilizan llaves `{}` para delimitar ámbitos léxicos. Los saltos de línea actúan como terminadores lógicos de expresiones cuando no hay operadores pendientes.
- **Comentarios:**
  - `//`: Comentario de una línea (en inglés).
  - `/* ... */`: Comentario multilínea.
  - `///`: Comentario de documentación (renderizable por el generador de docs).
- **Convención de Nombres:** `snake_case` para variables y funciones, `PascalCase` para tipos y estructuras, `SCREAMING_SNAKE_CASE` para constantes.

---

## 3. Ergonomía para Datos y Modelado Estadístico

### 3.1. Operador de Tubería (`|>`)
Permite encadenar transformaciones sobre flujos de datos sin anidamiento confuso de paréntesis:

```lang
let clean_summary = raw_dataset
    |> filter(row => row.age >= 18 && !is_na(row.income))
    |> mutate(log_income: log(row.income))
    |> group_by(:region)
    |> summarize(
        mean_log_income: mean(log_income),
        median_income: median(income),
        sample_size: count()
    );
```

La expresión `data |> func(arg)` se reescribe como `func(data, arg)`. Cuando se requiere inyectar el valor en una posición no inicial, se utiliza el marcador `_`: `y |> solve(A, _)`.

### 3.2. Operador de Fórmulas y Modelos (`~`)
La especificación de modelos estadísticos es un ciudadano de primera clase en la gramática de GHL, construyendo un árbol sintáctico abstracto (`Formula<Y, X>`) verificado estáticamente:

```lang
// Specify a linear mixed-effects model
let spec = response ~ treatment + age + (1 | subject_id);

// Fit model with compile-time formula validation
let model_fit = linear_model(spec, data: &patient_df)?;
```

### 3.3. Macros Empáticas para Desarrollo y Depuración
En honor a su espíritu cercano a las mascotas, GHL incluye macros nativas ergonómicas:
- `pounce!(condition, "message")`: Macro de aserción rápida. Si falla, emite un error estructurado con Kaomoji felino.
- `purr!("tracing info")`: Macro de traza detallada para optimizaciones del compilador y el planificador.

```lang
pounce!(sample_size >= 10, "Sample size is critically small for asymptotic estimation!");
```

### 3.4. Literales de Colecciones, Matrices y DataFrames

```lang
// 1D Vector literal (with native NA support)
let values = [1.0, 2.5, 3.8, NA, 5.2];

// 2D Matrix literal (semicolon separates rows)
let matrix_2x3 = mat[
    1.0, 2.0, 3.0;
    4.0, 5.0, 6.0
];

// Structural Record / Named Tuple
let record = { sample_id: "SMP-001", replicates: 4, p_value: 0.0042 };

// First-class Columnar DataFrame
let df = dataframe {
    id: [101, 102, 103],
    group: ["control", "treated", "treated"],
    score: [12.4, 18.9, 15.2]
};
```

---

## 4. Indexación Base 0 y Rebanado (Slicing)

Para preservar la máxima eficiencia con el hardware, la especificación de memoria de Apache Arrow y bibliotecas de bajo nivel (BLAS/LAPACK):
- **Base 0:** Los índices comienzan en `0`.
- **Sintaxis de Rangos:**
  - `v[0..5]`: Rango medio abierto $[0, 5)$ (índices 0, 1, 2, 3, 4).
  - `v[0..=5]`: Rango inclusivo $[0, 5]$.
  - `v[..]`: Todos los elementos.
  - `mat[0..2, :]`: Filas 0 y 1, todas las columnas.
- **Indexación Booleana (Masking) y Vectorial:**
  - `v[v > 0.0]`: Filtro lógico directo.
  - `v[[0, 2, 4]]`: Extracción por vector de índices.

---

## 5. Control de Flujo como Expresiones

### `if / else` Expresivo
```lang
let significance = if p_value < 0.01 {
    "Highly Significant"
} else if p_value < 0.05 {
    "Significant"
} else {
    "Not Significant"
};
```

### `match` con Desestructuración Exhaustiva
```lang
match optimization_result {
    Ok(estimate) => println("Estimated parameter: {estimate}"),
    Err(StatisticalError::SingularCovariance) => {
        println("(=^･ω･^=) Covariance matrix is singular. Consider regularization.");
    },
    Err(ComputeError::OutOfMemory) => {
        println("(ノ°□°)ノ Out of memory allocating computation buffer.");
    }
}
```

---

## 6. Funciones y Expresiones Lambda

```lang
// Explicitly typed function
fn log_posterior(theta: &Vector<f64>, data: &Matrix<f64>) -> f64 {
    let prior = Normal::standard().log_pdf(theta[0]);
    let likelihood = compute_likelihood(theta, data);
    prior + likelihood
}

// Terse lambdas for data transformations
let square = x => x * x;
let euclidean_norm = (x, y) => sqrt(x * x + y * y);
```

---

## 7. Léxico Formal: Especificación Canónica de Tokens, Palabras Clave y Verbos

Para evitar que futuros implementadores o agentes de IA inventen sintaxis inconsistente o palabras clave ambiguas, esta sección fija el inventario léxico normativo del lenguaje.

### 7.1. Palabras Clave Reservadas (Keywords)

| Categoría | Tokens / Palabras Clave | Descripción |
| :--- | :--- | :--- |
| **Declaraciones** | `fn`, `let`, `mut`, `const`, `struct`, `enum`, `trait`, `impl`, `type` | Definición de funciones, variables, constantes y estructuras de tipos |
| **Control de Flujo** | `if`, `else`, `match`, `for`, `in`, `while`, `return`, `break`, `continue` | Estructuras de control y salto condicional |
| **Módulos & Visibilidad**| `use`, `pub`, `mod`, `as`, `extern` | Gestión de espacios de nombres y visibilidad pública/privada |
| **Asincronía & Sistemas**| `async`, `await` | Primitivas para I/O asíncrono y servidores |
| **Literales Booleanos & NA**| `true`, `false`, `NA` | Constantes de verdad y valor ausente base |
| **Constructores Especiales**| `dataframe`, `mat`, `col` | Constructores sintácticos de tablas y matrices |

### 7.2. Operadores y Puntuación

| Token | Nombre Formal | Significado en GHL |
| :--- | :--- | :--- |
| `\|>` | Pipe Operator | Pasa el operando izquierdo como 1er argumento a la función derecha |
| `~` | Modeling Operator | Construye una fórmula estadística AST (`Formula<Y, X>`) |
| `_` | Placeholder | Marcador de posición para argumentos en pipes o wildcards en `match` |
| `\\` | Matrix Solve | Resuelve el sistema lineal $A \cdot x = b$ mediante $A \backslash b$ |
| `.*`, `.+`, `.-`, `./` | Element-wise Matrix Ops | Operaciones elemento a elemento (broadcasting explícito) |
| `+`, `-`, `*`, `/`, `%` | Arithmetic Operators | Aritmética estándar de punto flotante y enteros |
| `^` | Power Operator | Exponenciación numérica ($a^b$) |
| `==`, `!=`, `<`, `<=`, `>`, `>=` | Comparison Operators | Comparaciones con lógica Kleene en presencia de `NA` |
| `&&`, `\|\|`, `!` | Logical Operators | Conjunción, disyunción y negación booleana trivalente |
| `:` | Colon | Anotación de tipos (`x: f64`) o prefijo de motivo `NA` (`NA:NoResponse`) |
| `::` | Path Separator | Separador de módulos y tipos asociados (`std::stats::Normal`) |
| `->` | Arrow Return | Declara el tipo de retorno de una función |
| `=>` | Fat Arrow | Cuerpo de lambda o brazo de coincidencia en `match` |
| `?` | Try / Propagate | Propaga errores tempranamente (`Result::Err`) |
| `..`, `..=` | Range Operators | Rango semi-abierto $[a, b)$ y rango inclusivo $[a, b]$ |
| `(`, `)`, `[`, `]`, `{`, `}` | Enclosures | Paréntesis, corchetes y llaves delimitadoras |
| `,`, `;` | Separators | Separador de elementos y terminador de sentencias |

### 7.3. Verbos Canónicos para Transformación de Datos (`std::dataframe`)

Toda operación de manipulación tabular en GHL debe ceñirse a este conjunto estándar de verbos (inspirados en el diseño funcional moderno):

```lang
// Canonical DataFrame verbs pipeline:
df  |> filter(col("age") > 18)                           // Keep rows matching predicate
    |> select(["patient_id", "treatment", "outcome"])     // Project columns
    |> drop(["temporary_tag"])                            // Remove columns
    |> mutate(log_outcome: log(col("outcome")))           // Create or transform columns
    |> arrange(by: [col("treatment"), desc(col("age"))])  // Sort rows
    |> group_by(["treatment"])                            // Partition data
    |> summarize([                                        // Aggregate partitions
        col("outcome").mean().as("mean_resp"),
        count().as("n_samples")
    ])
    |> join(demographics_df, on: "patient_id", kind: Inner) // Relational join
    |> distinct(by: ["patient_id"])                       // Deduplicate rows
    |> pivot_wider(names_from: "visit", values_from: "score") // Reshape wide
    |> impute(col("log_outcome"), strategy: Mean, only_for: [NAReason::SensorDropout]);
```

### 7.4. Verbos Canónicos para Inferencia y Modelado (`std::stats`)

| Verbo Canónico | Propósito | Ejemplo |
| :--- | :--- | :--- |
| `fit(formula, data)` | Ajusta el estimador estadístico a los datos | `let model = OLS::fit(y ~ x1 + x2, data: &df)?;` |
| `predict(model, newdata)` | Genera valores predichos e intervalos | `let y_hat = model.predict(&test_df);` |
| `residuals(model)` | Extrae residuos ordinarios o estandarizados | `let e = model.residuals();` |
| `summary(model)` | Genera el informe diagnóstico completo | `model.summary();` |
| `sample(dist, &mut rng)` | Obtiene una muestra escalar aleatoria | `let draw = dist.sample(&mut rng);` |
| `sample_n(dist, n, &mut rng)`| Obtiene un vector de $N$ observaciones | `let draws = dist.sample_n(1_000, &mut rng);` |
| `log_pdf(dist, x)` / `pdf(dist, x)`| Densidad de probabilidad o masa | `let lp = dist.log_pdf(1.5);` |
| `cdf(dist, x)` | Función de distribución acumulada | `let prob = dist.cdf(1.96);` |
| `quantile(dist, p)` | Función cuantil / inversa de CDF | `let critical = dist.quantile(0.975);` |

### 7.5. Macros Canónicas

- `pounce!(condition, "message")`: Aserción que emite un diagnóstico felino `(・`ω´・)` si la condición no se cumple.
- `purr!("message")`: Registro de trazas de rendimiento y optimizaciones en modo verbose.
- `println!("message {var}")`: Salida estándar formateada con interpolación de cadenas.
- `panic!("message")`: Aborta inmediatamente el hilo con un Kaomoji de colapso informático `(╯°□°)╯︵ ┻━┻`.


