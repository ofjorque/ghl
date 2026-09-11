# Matriz Comparativa Multidimensional: GHL vs R, Python y Julia

Esta tabla resume las diferencias clave de arquitectura, rendimiento, semántica, experiencia de usuario y diagnósticos entre **GHL** (*Generalized Hypothesis Language*) y los tres referentes consolidados de la computación científica.

---

## Tabla Resumen de Características

| Dimensión / Característica | **GHL (`.gh` / `.ghl`)** | **R (Base + Tidyverse)** | **Python (NumPy + PyData)** | **Julia** |
| :--- | :--- | :--- | :--- | :--- |
| **Paradigma Principal** | Expresiones / Funcional / Imperativo | Funcional / Vectorizado / S3/S4 | Multiparadigma / OOP | Múltiple Despacho / Funcional |
| **Modelo de Compilación** | AOT nativo (Cranelift) + JIT ligero (REPL, también Cranelift) | Interpretado (Bytecode eval) | Interpretado (CPython bytecode) | JIT intensivo (LLVM en runtime) |
| **Problema de los 2 Lenguajes** | **Resuelto** (código nativo puro) | No resuelto (depende de C/C++/Rust) | No resuelto (depende de C/C++/Rust) | **Resuelto** (código puro es rápido) |
| **Tiempo de Arranque / TTFX** | **Instantáneo** (< 20 ms, medido en §"Resultados Empíricos") | ~111 ms medido (Base R) | ~35 ms medido | **~165 ms medido en "hello world"**; cargas de paquetes pesados (`DataFrames.jl`, `Plots.jl`) o la primera invocación de una función con tipos nuevos puede escalar a **0.5s-15s** (no reproducido en esta suite, ver discusión cualitativa en [vs-julia.md](vs-julia.md) §2.A; sí medimos el costo de `using DataFrames, CSV` en la Suite 02, ver más abajo) |
| **Gestión de Memoria** | ARC + CoW + Arenas (Sin Tracing GC) | Tracing GC (generacional simple) | Conteo de ref + Tracing GC cíclico | Tracing GC multihilo |
| **Pausas de GC en Cómputo** | **0 ms (Cero pausas impredecibles)**| Frecuentes en creación de objetos | Pausas periódicas moderadas | Pausas de GC que afectan latencia |
| **Manejo de Datos Faltantes** | `NA` nativo con lógica Kleene 3-val | `NA` nativo (múltiples tipos) | Inconsistente (`None`, `nan`, `pd.NA`) | `missing` con lógica 3-val |
| **DataFrames Nativos** | Sí (Formato columnar Apache Arrow) | Sí (`data.frame`, `data.table`) | No (Librerías externas: Pandas/Polars)| No (Librería externa: `DataFrames.jl`)|
| **Fórmulas Estadísticas (`~`)** | Sintaxis nativa de primer nivel | Sintaxis nativa (`formula`) | Librería externa (`Formulaic`/`Patsy`)| Librería externa (`StatsModels.jl`) |
| **Indexación Base** | **0-based** (compatible C / Arrow) | 1-based | 0-based | 1-based |
| **Taxonomía de Diagnósticos** | **Dual: `[Compute]` vs `[Statistical]`** | Mezcla confusa en stderr | Excepciones genéricas / Traceback | Errores de tipo / MethodErrors |
| **Experiencia de Usuario / UX**| **Empática con Kaomojis** (`ฅ^•ﻌ•^ฅ`) | Clásica / sobria | Clásica / técnica | Clásica / académica |
| **Concurrencia / Paralelismo** | Multi-hilo nativo, sin GIL, SIMD | Monohilo (procesos bifurcados) | Monohilo en Python puro (GIL) | Multi-hilo nativo (con locks/tasks) |
| **Generación de Binarios** | Binarios pequeños y autónomos (<15 MB)| Difícil (requiere empaquetar R) | Difícil (PyInstaller / enorme peso) | Muy complejo (System images >300 MB)|
| **Interoperabilidad Arrow** | Zero-Copy nativa | Mediante paquete `arrow` | Mediante `pyarrow` | Mediante `Arrow.jl` |

---

## Resultados Empíricos Medidos (Fase 10)

Mediciones con `hyperfine`, **30 iteraciones** por celda, Windows x86_64. Se muestra la
librería base de cada lenguaje y, entre paréntesis, la variante idiomática optimizada
cuando difiere (`Polars`/`data.table`/`DataFrames.jl`/`Numba`) — ver metodología en
[results/summary.md](../results/summary.md).

| Métrica / Benchmark | GHL | Python | R | Julia | Resultado |
| :--- | :---: | :---: | :---: | :---: | :--- |
| **Startup / TTFX** | **5.3 ms** (AOT) / 10.5 ms (Interp) | 35.2 ms | 111.7 ms | 165.1 ms | GHL gana, 6.6x-31.1x |
| **Dot Product ($10^7$ floats)** | **54.7 ms** | 224.3 ms (NumPy, ya usa BLAS) | 311.5 ms (BLAS de referencia) | 412.8 ms (ya usa BLAS) | GHL gana, 4.1x-7.6x |
| **DataFrames (1M filas)** | 643.0 ms | 1,219.7 ms Pandas / **310.5 ms Polars** | 3,611.5 ms base / 473.2 ms data.table | 1,461.2 ms streaming / 5,699.3 ms DataFrames.jl | **GHL PIERDE**: Polars-Python es 2.07x más rápido y data.table-R 1.36x más rápido que GHL |
| **Gibbs Sampler (100 iter)** | **21.4 ms** | 397.9 ms NumPy / 786.9 ms Numba (más lento) | 138.1 ms | 559.5 ms / 553.1 ms @inbounds (sin cambio) | GHL gana con margen amplio, 6.5x-36.8x |

El hallazgo central de esta ronda: **GHL no es universalmente el más rápido.** En la
Suite de DataFrames, comparado contra el estado del arte real de cada ecosistema
(no contra Pandas/`aggregate`/un parser artesanal), GHL queda detrás de Polars-Python
y data.table-R. Es el más rápido en arranque, álgebra vectorial y bucles iterativos
(MCMC), pero no en ingestión/agregación de DataFrames a esta escala.

*Ver informe detallado, incluyendo notas de honestidad por suite, en [benchmarks/results/summary.md](../results/summary.md).*
