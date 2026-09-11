# Matriz Comparativa Multidimensional: GHL vs R, Python y Julia

Esta tabla resume las diferencias clave de arquitectura, rendimiento, semántica, experiencia de usuario y diagnósticos entre **GHL** (*Generalized Hypothesis Language*) y los tres referentes consolidados de la computación científica.

---

## Tabla Resumen de Características

| Dimensión / Característica | **GHL (`.gh` / `.ghl`)** | **R (Base + Tidyverse)** | **Python (NumPy + PyData)** | **Julia** |
| :--- | :--- | :--- | :--- | :--- |
| **Paradigma Principal** | Expresiones / Funcional / Imperativo | Funcional / Vectorizado / S3/S4 | Multiparadigma / OOP | Múltiple Despacho / Funcional |
| **Modelo de Compilación** | AOT nativo (LLVM) + JIT ligero (REPL) | Interpretado (Bytecode eval) | Interpretado (CPython bytecode) | JIT intensivo (LLVM en runtime) |
| **Problema de los 2 Lenguajes** | **Resuelto** (código nativo puro) | No resuelto (depende de C/C++/Rust) | No resuelto (depende de C/C++/Rust) | **Resuelto** (código puro es rápido) |
| **Tiempo de Arranque / TTFX** | **Instantáneo** (< 20 ms) | Rápido (~50 ms) | Moderado (~80-150 ms) | **Lento a muy lento** (0.5s - 15s) |
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

Mediciones automatizadas con `hyperfine` en entorno Windows x86_64:

| Métrica / Benchmark | GHL | Python | R | Julia | Ventaja GHL |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Startup / TTFX** | **6.7 ms** (AOT) / **10.7 ms** (Interp) | 34.8 ms | 115.1 ms | 166.4 ms | **5.2x a 24.8x más rápido** |
| **Dot Product ($10^7$ floats)** | **52.7 ms** | 215.8 ms | 301.5 ms | 398.8 ms | **4.1x a 7.6x más rápido** |
| **DataFrames (1M filas)** | **658.8 ms** | 1,223 ms | 3,656 ms | 1,505 ms | **1.9x a 5.6x más rápido** |
| **Gibbs Sampler (100 iter)** | **21.1 ms** | 402.0 ms | 139.5 ms | 575.5 ms | **6.6x a 27.3x más rápido** |

*Ver informe detallado en [benchmarks/results/summary.md](file:///o:/Documentos/Rust%20Project/ghl/benchmarks/results/summary.md).*
