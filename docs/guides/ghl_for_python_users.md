# Guía de Migración para Usuarios de Python / Pandas a GHL

Bienvenido a **GHL** (*Generalized Hypothesis Language*). Si estás acostumbrado a desarrollar modelos y pipelines de análisis de datos en **Python** con bibliotecas como **Pandas**, **Polars**, **NumPy**, **SciPy** y **Statsmodels**, esta guía te explicará cómo traducir tus flujos de trabajo al modelo computacional de GHL.

GHL combina la expresividad matemática de NumPy y la flexibilidad columnar de Polars en un lenguaje compilado de forma nativa (JIT/AOT), **sin Global Interpreter Lock (GIL)**, sin pausas de *garbage collection*, y con verificación estática de tipos.

---

## 1. Comparativa de DataFrames: Pandas / Polars vs. GHL

En Python existen dos paradigmas principales para datos tabulares:
- **Pandas:** Basado en objetos mutables en memoria, índices de filas explícitos (`df.index`), y tipado dinámico respaldado por arrays de NumPy o extensiones de C.
- **Polars:** Basado en el formato Apache Arrow, ejecución paralela multi-núcleo y optimizador de consultas diferidas (*lazy evaluation*).

**GHL unifica lo mejor de ambos mundos:**
1. **Memoria Columnar Apache Arrow:** Almacenamiento nativo contiguo en memoria, ideal para vectorización SIMD.
2. **Evaluación Eager y Lazy:** Soporte para ejecución interactiva inmediata (`read_csv`, `dataframe { ... }`) y planes de ejecución perezosos optimizados (`scan_csv`, `scan_parquet`, `collect`, `explain`).
3. **Sin Índices de Fila Complejos:** Al igual que en Polars y R, las tablas son colecciones de vectores columnares uniformes, eliminando la sobrecarga y ambigüedades del `MultiIndex` de Pandas.

### Tabla de Equivalencias de Operaciones Comunes

| Operación | Python (Pandas) | Python (Polars) | GHL (Sintaxis Canónica) |
| :--- | :--- | :--- | :--- |
| **Carga de CSV** | `df = pd.read_csv("data.csv")` | `df = pl.read_csv("data.csv")` | `let df = read_csv("data.csv");` |
| **Carga Perezosa (Lazy)** | *No soportado de forma nativa* | `df = pl.scan_csv("data.csv")` | `let plan = scan_csv("data.csv");` |
| **Filtrado de Filas** | `df[df["age"] > 30]` | `df.filter(pl.col("age") > 30)` | `df \|> filter(age > 30)` |
| **Creación de Columnas** | `df["ratio"] = df["a"] / df["b"]` | `df.with_columns((pl.col("a") / pl.col("b")).alias("ratio"))` | `df \|> mutate(ratio = a / b)` |
| **Selección de Columnas** | `df[["name", "age"]]` | `df.select(["name", "age"])` | `df \|> select(name, age)` |
| **Agrupamiento y Agregación**| `df.groupby("team")["score"].mean()` | `df.group_by("team").agg(pl.col("score").mean())` | `df \|> group_by(team) \|> summarize(score = mean(score))` |
| **Ordenamiento** | `df.sort_values(by="score", ascending=False)` | `df.sort("score", descending=True)` | `df \|> arrange(desc(score))` |
| **Cálculo de Frecuencias** | `df["team"].value_counts()` | `df["team"].value_counts()` | `df \|> count(team)` |
| **Valores Distintos** | `df.drop_duplicates(subset=["id"])` | `df.unique(subset=["id"])` | `df \|> distinct(id)` |
| **Uniones Relacionales** | `pd.merge(a, b, on="id", how="inner")` | `a.join(b, on="id", how="inner")` | `inner_join(a, b, "id")` |

---

## 2. Álgebra Lineal: NumPy / SciPy vs. GHL

En Python, el álgebra lineal descansa en NumPy (`numpy.linalg`) y SciPy (`scipy.linalg`), llamando a rutinas BLAS/LAPACK subyacentes. En GHL, el álgebra lineal está integrada en el lenguaje y el compilador, con algoritmos de factorización de alto rendimiento optimizados por **faer** y soporte para tipos matriciales nativos (`Matrix`).

### Equivalencias de Operaciones y Factorizaciones

| Operación de Álgebra Lineal | Python (NumPy / SciPy) | GHL (Sintaxis Canónica) |
| :--- | :--- | :--- |
| **Matriz Identidad** | `np.eye(n)` | `identity(n)` *(o `eye(n)`)* |
| **Matriz de Ceros** | `np.zeros((rows, cols))` | `zeros(rows, cols)` |
| **Producto Interno (Dot Product)** | `np.dot(u, v)` o `u @ v` | `dot(u, v)` *(vectorizado con AVX2/NEON)* |
| **Multiplicación Matricial** | `A @ B` | `A * B` |
| **Transposición de Matriz** | `A.T` | `transpose(A)` *(o `t(A)`)* |
| **Resolución de Sistemas Lineales ($Ax = b$)** | `np.linalg.solve(A, b)` | `A \ b` *(operador de barra invertida de primer orden)* |
| **Inversa de Matriz ($A^{-1}$)** | `np.linalg.inv(A)` | `inv(A)` |
| **Descomposición de Cholesky ($A = L L^T$)** | `np.linalg.cholesky(A)` | `cholesky(A)` |
| **Descomposición QR ($A = Q R$)** | `Q, R = np.linalg.qr(A)` | `let d = qr(A); let Q = qr_q(d); let R = qr_r(d);` |
| **Valores y Vectores Propios** | `eigenvals, eigenvecs = np.linalg.eigh(A)` | `let d = eigen(A); let vals = eigen_values(d);` |
| **Descomposición en Valores Singulares (SVD)** | `U, S, Vt = np.linalg.svd(A)` | `let d = svd(A); let s = svd_s(d);` |

### Ejemplo: Resolución de Ecuaciones Normales

#### En Python (NumPy):
```python
import numpy as np

# Synthetic design matrix and response vector
X = np.array([[1.0, 2.0], [1.0, 3.0], [1.0, 4.0], [1.0, 5.0]])
y = np.array([2.1, 3.9, 6.2, 7.8])

# Normal equations: (X^T * X) * beta = X^T * y
XtX = X.T @ X
Xty = X.T @ y

# Solve via linear solver
beta = np.linalg.solve(XtX, Xty)
```

#### En GHL:
```ghl
// Define matrices with native syntax
let X = mat [
    1.0, 2.0 ;
    1.0, 3.0 ;
    1.0, 4.0 ;
    1.0, 5.0
];
let y = [2.1, 3.9, 6.2, 7.8];

// Normal equations solved via first-class backslash operator
let XtX = transpose(X) * X;
let Xty = transpose(X) * y;
let beta = XtX \ Xty;
```

---

## 3. Paradigma: Métodos de Objetos vs. Expresiones Funcionales Canalizadas

Un cambio conceptual fundamental al migrar de Pandas a GHL es el estilo de composición:

### El Enfoque de Pandas: Orientado a Objetos y Métodos Encadenados
En Pandas, todo el análisis se centra en llamadas a métodos de la clase `DataFrame`, intercalando parámetros que frecuentemente mutan estados o requieren gestionar argumentos como `inplace=True`, `axis=0`, o `reset_index()`:

```python
# Pandas method chaining
result = (
    df[df["status"] == "active"]
    .assign(ratio=lambda d: d["score"] / d["max_score"])
    .groupby("category", as_index=False)["ratio"]
    .agg(["mean", "std"])
    .rename(columns={"mean": "avg_ratio"})
    .sort_values(by="avg_ratio", ascending=False)
)
```

### El Enfoque de GHL: Composición Funcional Transparente
En GHL, los datos fluyen a través de funciones y verbos puros mediante el operador pipe (`|>`). No existe la ambigüedad de mutación accidental, no existen índices ocultos, y las expresiones en las funciones de transformación se ejecutan directamente en **contexto de columna**:

```ghl
// GHL functional pipeline
let result = df
    |> filter(status == "active")
    |> mutate(ratio = score / max_score)
    |> group_by(category)
    |> summarize(
        avg_ratio = mean(ratio),
        sd_ratio = std_dev(ratio)
    )
    |> arrange(desc(avg_ratio));
```

---

## 4. Concurrencia y Paralelismo: Olvídate del GIL

En Python, el *Global Interpreter Lock* (GIL) impide la ejecución simultánea de código en múltiples núcleos de CPU en un solo proceso. Librerías como `multiprocessing` o `concurrent.futures` deben recurrir a la serialización profunda (*pickling* de objetos) y copias en memoria entre procesos.

En GHL:
1. **Multi-threading Real:** GHL ejecuta hilos nativos del sistema operativo sin GIL.
2. **Iteradores Paralelos Reproducibles:** Mediante el método `.par_iter()`, puedes paralelizar transformaciones, simulaciones Monte Carlo o muestreos Bootstrap en todos los núcleos de la máquina con cero configuración:

```ghl
// Parallel simulation across available CPU threads
let bootstrapped_means = (0..1_000)
    .par_iter()
    .map(|seed_offset| {
        // Reproducible calculation per iteration
        let sample = random_normal(100, 50.0, 10.0);
        mean(sample)
    })
    .collect();
```
