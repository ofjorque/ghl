# Suite 01: Álgebra Vectorial y Matricial en GHL

- **Área:** Cómputo Numérico de Bajo Nivel / BLAS / SIMD
- **Objetivo:** Evaluar la eficiencia en el acceso a memoria continua, optimizaciones SIMD y enlace con bibliotecas de álgebra lineal de alto rendimiento.

---

## 1. Casos de Prueba Especificados

### Caso 1.1: Producto Escalar Vectorial (Dot Product)
- **Operación:** $\sum_{i=1}^{N} a_i \cdot b_i$ para dos vectores de $N = 10^7$ flotantes de 64 bits (`f64`).
- **Aspecto Evaluado:** Vectorización automática SIMD (AVX2/AVX-512/Neon), desenrollado de bucles y saturación de ancho de banda de memoria RAM/L3.

### Caso 1.2: Multiplicación Matricial Densa ($C = A \times B$)
- **Operación:** Multiplicación de dos matrices cuadradas de $N \times N$ ($N = 2000$) en doble precisión (`f64`).
- **Aspecto Evaluado:** Eficiencia del planificador multinúcleo e integración con BLAS (OpenBLAS/MKL o kernel nativo optimizado por bloques).

### Caso 1.3: Descomposición de Cholesky
- **Operación:** Factorización $A = L L^T$ de una matriz simétrica definida positiva de $2000 \times 2000$.
- **Aspecto Evaluado:** Rendimiento de LAPACK y estabilidad numérica.

### Caso 1.4: Fusión de Operaciones No Lineales Elemento a Elemento
- **Operación:** $y_i = \log(1.0 + \exp(-|x_i|)) + \sin(x_i)$ para $N = 5 \times 10^7$ elementos.
- **Aspecto Evaluado:** Capacidad del compilador para fusionar operaciones elementales en un solo recorrido de memoria sin asignar buffers intermedios en el heap.

---

## 2. Implementaciones de Referencia para la Comparativa

### R (Base + BLAS)
```r
# Case 1.1: Dot product
a <- runif(1e7); b <- runif(1e7)
res <- sum(a * b) # Note: creates an intermediate vector in RAM

# Case 1.4: Element-wise fusion
x <- runif(5e7)
y <- log(1.0 + exp(-abs(x))) + sin(x) # Creates multiple temporary vectors in RAM
```

### Python (NumPy)
```python
import numpy as np

# Case 1.1: Dot product
a = np.random.rand(10_000_000); b = np.random.rand(10_000_000)
res = np.dot(a, b)

# Case 1.4: Element-wise fusion
x = np.random.rand(50_000_000)
# Standard NumPy generates 4 huge temporary arrays (~400 MB each):
y = np.log1p(np.exp(-np.abs(x))) + np.sin(x)
```

### Julia
```julia
# Case 1.1: Dot product
a = rand(10_000_000); b = rand(10_000_000)
res = a' * b

# Case 1.4: Element-wise fusion
x = rand(50_000_000)
y = @. log(1.0 + exp(-abs(x))) + sin(x) # Full kernel fusion
```

### GHL (`.gh`)

GHL no tiene sintaxis de método (`.foo()`): solo funciones libres y pipes (`|>`), y las
lambdas se escriben `\param -> expr` (no `param => expr`). `random_uniform`, `dot` y `map`
son funciones libres de la biblioteca estándar, no métodos de `Vector`.

```lang
// Case 1.1: Dot Product (Compiles to direct vector register SIMD instructions)
let a = random_uniform(10_000_000);
let b = random_uniform(10_000_000);
let res = dot(a, b);

// Case 1.4: Element-wise fusion with zero intermediate heap allocations
let x = random_uniform(50_000_000);
let y = x |> map(\xi -> log(1.0 + exp(-abs(xi))) + sin(xi));
```
