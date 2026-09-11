# Análisis Comparativo: GHL vs R

- **Área:** Análisis de Lenguaje / Paradigmas
- **Referente:** R (GNU R 4.x + Ecosistema Tidyverse & CRAN)

---

## 1. El Legado y las Fortalezas de R

R (creado por Ross Ihaka y Robert Gentleman como evolución de S) fue concebido explícitamente para **pensar con datos**. Sus principales ventajas históricas son:
- **Ergonomía Estadística Inigualable:** La notación de fórmulas (`y ~ x1 + x2`) y el concepto de `data.frame` son primitivas centrales del lenguaje, no librerías de terceros.
- **Manejo Nativo de Datos Ausentes:** Tipos `NA` especializados (`NA_real_`, `NA_integer_`, etc.) con propagación estadística coherente.
- **Riqueza de CRAN:** Décadas de investigación estadística de vanguardia publicadas como paquetes listos para usar.

---

## 2. Los Cuellos de Botella Críticos de R

### A. El Castigo de los Bucles Escalares
En R, un bucle `for` tradicional sobre millones de elementos puede ser **100 a 300 veces más lento** que en C o Rust. Esto obliga a los desarrolladores de R a:
- Forzar la vectorización mental constante (a menudo haciendo el código menos legible).
- Escribir extensiones en C/C++ usando `Rcpp` o `cpp11`, dividiendo el proyecto en dos lenguajes distintos con herramientas de compilación complejas.

### B. Sobrecarga Oculta de Memoria por Copy-on-Write Indiscriminado
El modelo de memoria de R intenta ser funcional e inmutable mediante Copy-on-Write (CoW). Sin embargo, debido a la falta de análisis estático y a la complejidad del sistema de tipos S3/S4, R frecuentemente duplica estructuras gigantes en memoria de manera silenciosa:
```r
# In R: Mutating a single cell in a 10 GB data frame
# often duplicates the full 10 GB in RAM silently:
df[1, "score"] <- 42.0
```

### C. Arquitectura Monohilo
El intérprete de R es intrínsecamente monohilo. Para paralelizar tareas, los paquetes tradicionales bifurcan procesos completos (`fork` o sockets locales), multiplicando el consumo de memoria por el número de núcleos de CPU disponibles.

---

## 3. Cómo Supera GHL a R

1. **Rendimiento Nativo sin Salir del Lenguaje:** Los bucles, recursiones y algoritmos complejos escritos en GHL se compilan a código máquina optimizado con vectorización automática SIMD. No se necesita `Rcpp`.
2. **Mutación Inteligente In-Place:** Gracias al conteo de referencias unívoco (`ref_count == 1`), modificar una columna o celda en un DataFrame de 10 GB es instantáneo y no duplica la memoria en RAM.
3. **Paralelismo de Memoria Compartida:** Ejecución de modelos en paralelo a través de todos los hilos de CPU compartiendo las mismas matrices de datos sin duplicación de procesos.
4. **Preservación de la Ergonomía:** Mantiene la sintaxis de fórmulas (`~`), el operador de tubería (`|>`) y la lógica ternaria formal de `NA`.
5. **Diagnósticos Claros con Kaomojis:** Mientras R arroja advertencias crípticas en rojo (`Warning: glm.fit: fitted probabilities numerically 0 or 1 occurred`), GHL emite un diagnóstico empático y preciso:
   ```
   (ФωФ) [Statistical Warning SW014]: Perfect Separation Detected in Logistic Fit
     = Note: Fitted probabilities evaluated to 0.0 or 1.0. Coefficients may diverge to infinity.
   ```

---

## 4. Resultados Empíricos (Benchmarks Fase 10)

Resultados medidos en igualdad de condiciones sobre Windows x86_64:

| Prueba | GHL | R 4.6.1 | Ventaja de GHL |
| :--- | :---: | :---: | :---: |
| **Startup (TTFX)** | **6.7 ms** (AOT) / **10.7 ms** (Interp) | 115.1 ms | **10.7x a 17.1x más rápido** |
| **Dot Product ($10^7$ floats)** | **52.7 ms** | 301.5 ms | **5.7x más rápido** |
| **DataFrames (1M filas CSV, group-by, agg)** | **658.8 ms** | 3,656 ms | **5.55x más rápido** |
| **Gibbs Sampler (100 iteraciones)** | **21.1 ms** | 139.5 ms | **6.62x más rápido** |
