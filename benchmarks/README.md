# Banco de Pruebas y Comparativas de GHL

Este directorio contiene las especificaciones de rendimiento, comparativas arquitectónicas detalladas y suites reproducibles para evaluar **GHL** (*Generalized Hypothesis Language*, `.gh` / `.ghl`) frente a los tres grandes referentes de la computación estadística y científica: **R**, **Python** y **Julia**.

---

## Objetivos del Banco de Pruebas

1. **Rigor Cuantitativo:** Medir de manera imparcial y reproducible métricas clave de rendimiento:
   - Tiempo de ejecución en cómputo numérico puro (bucles de usuario vs librerías C).
   - Huella de memoria máxima (Peak RSS) y asignaciones en heap.
   - Tiempo de respuesta interactiva y arranque (*Time to First Execution* / TTFX).
   - Rendimiento en operaciones sobre marcos de datos (DataFrames).
2. **Comparativa Cualitativa:** Identificar las fortalezas arquitectónicas y los compromisos de diseño de cada entorno.

---

## Estructura de este Directorio

- [**`methodology.md`**](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/methodology.md): Protocolo de medición estricto, configuración de hardware, aislamiento de CPU y criterios de imparcialidad.
- [**`comparisons/`**](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/comparisons/):
  - [`comparison-matrix.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/comparisons/comparison-matrix.md): Matriz comparativa multidimensional (características, paradigmas, rendimiento).
  - [`vs-r.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/comparisons/vs-r.md): Análisis exhaustivo frente a R.
  - [`vs-python.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/comparisons/vs-python.md): Análisis exhaustivo frente a Python (NumPy, Pandas, PyTorch).
  - [`vs-julia.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/comparisons/vs-julia.md): Análisis exhaustivo frente a Julia.
- [**`suites/`**](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/suites/):
  - [`01-vector-and-matrix-math.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/suites/01-vector-and-matrix-math.md): Álgebra matricial, descomposiciones y operaciones vectoriales.
  - [`02-dataframe-operations.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/suites/02-dataframe-operations.md): Ingestión de CSV/Parquet, joins, group-by y filtrado.
  - [`03-statistical-modeling.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/suites/03-statistical-modeling.md): MCMC, ajuste de modelos lineales generalizados (GLM) y bootstrap.
  - [`04-runtime-characteristics.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/suites/04-runtime-characteristics.md): Tiempo de arranque, tamaño del binario y latencia de compilación.
