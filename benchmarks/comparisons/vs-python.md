# Análisis Comparativo: GHL vs Python

- **Área:** Análisis de Lenguaje / Paradigmas
- **Referente:** Python 3.12+ (Ecosistema PyData: NumPy, Pandas, Polars, Scikit-learn, PyTorch)

---

## 1. El Ecosistema PyData y el Éxito de Python

Python se ha consolidado como el estándar de facto de la industria del aprendizaje automático gracias a:
- Ecosistema inmenso y maduro (`scikit-learn`, `PyTorch`, `HuggingFace`).
- Sintaxis accesible y legible para desarrolladores de cualquier disciplina.
- Excelente rol como lenguaje de integración (*glue language*).

---

## 2. Las Limitaciones Arquitectónicas de Python

### A. La Realidad del "Problema de los Dos Lenguajes"
Python en sí mismo es un intérprete lento. Prácticamente **ningún cómputo numérico pesado ocurre en código Python puro**:
- `NumPy` está escrito en C.
- `PyTorch` y `TensorFlow` están escritos en C++.
- `Polars` está escrito en Rust.
- `SciPy` combina Fortran y C.

Cuando un investigador o estadístico desea implementar un algoritmo novedoso que no se reduce a operaciones vectoriales ya precompiladas en C, el rendimiento colapsa. Se recurre entonces a parches complejos como Cython o Numba, que añaden dependencias frágiles y complican el despliegue.

### B. El Bloqueo Global del Intérprete (GIL)
El intérprete estándar CPython restringe la ejecución de múltiples hilos en paralelo para código de usuario. El uso de `multiprocessing` requiere serialización costosa (`pickle`) de grandes arrays entre procesos aislados.

### C. Caos de Datos Faltantes (Missing Values)
Python carece de un tipo nativo unificado para datos faltantes:
- `None`: Objeto general de Python (provoca que los arrays de NumPy se conviertan en vectores lentos de punteros `object`).
- `np.nan`: Número en coma flotante IEEE-754. Si una columna de enteros tiene un dato faltante, NumPy históricamente la convierte a `float64` forzosamente.
- `pd.NA`: Intento tardío de Pandas de introducir un tipo faltante enmascarado.

---

## 3. Cómo Supera GHL a Python

1. **Un Solo Lenguaje para Todo el Stack:** El usuario escribe algoritmos en alto nivel y se compilan directamente a instrucciones nativas vectorizadas SIMD sin recurrir a C, Cython o Rust.
2. **Cero GIL y Concurrencia de Memoria Compartida:** Soporte nativo para paralelismo multihilo en estructuras de datos compartidas sin sobrecargas de serialización.
3. **Manejo de `NA` sin Corrupción de Tipos:** Soporte formal para enteros, booleanos y cadenas con valores faltantes mediante bitmasks Arrow nativos, sin degradar enteros a flotantes.
4. **Despliegue Limpio y Determinista:** Generación de binarios ejecutables autónomos de pocos megabytes, eliminando la pesadilla de entornos virtuales corruptos (`venv`) y dependencias C-ABI incompatibles.
5. **Diagnósticos Claros:** En lugar de tracebacks de 40 líneas de excepciones genéricas de Python, GHL señala inmediatamente si el error es de cómputo (`[Compute Error]`) o estadístico (`[Statistical Error]`) con Kaomojis empáticos.

---

## 4. Resultados Empíricos (Benchmarks Fase 10)

Resultados medidos con `hyperfine`, 30 iteraciones, Windows x86_64, frente a Python 3.14
(NumPy 2.5, Pandas 3.0, **Polars 1.44**, **Numba 0.67**). methodology.md §1.1 promete
evaluar Polars y Numba explícitamente, así que se incluyen como variantes propias en
vez de comparar sólo contra Pandas base:

| Carga de Trabajo | GHL | Python (PyData) | Resultado |
| :--- | :---: | :---: | :--- |
| **Startup / TTFX** | **5.3 ms** (AOT) / 10.5 ms (Interp) | 35.2 ms | GHL 3.3x-6.6x más rápido |
| **Dot Product ($10^7$ floats)** | **54.7 ms** | 224.3 ms (NumPy, ya vía BLAS) | GHL 4.10x más rápido |
| **DataFrames (1M filas CSV, group-by, agg)** | 643.0 ms | 1,219.7 ms (Pandas) / **310.5 ms (Polars)** | **Polars-Python es 2.07x más rápido que GHL** — GHL sólo gana contra Pandas (1.90x), no contra Polars |
| **Gibbs Sampler (100 iteraciones)** | **21.4 ms** | 397.9 ms (NumPy) / 786.9 ms (Numba JIT) | GHL 18.6x más rápido que NumPy; Numba resulta **más lento** que NumPy puro aquí (el costo de importar/compilar JIT supera lo que ahorra en un bucle de 300 iteraciones totales) |

**Nota de honestidad:** en DataFrames, la comparación justa (Polars vs Polars, ya que
ambos motores son esencialmente el mismo `polars-core` de Rust) muestra a GHL
*perdiendo*, no ganando — ver el detalle de por qué en la sección "Suite 02" de
[../results/summary.md](../results/summary.md).
