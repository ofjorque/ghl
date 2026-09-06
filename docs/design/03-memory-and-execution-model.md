# RFC 03: Modelo de Memoria y Ejecución de GHL

- **Estado:** Propuesto
- **Área:** Runtime / Arquitectura de Memoria / Compilador
- **Audiencia:** Diseñadores del runtime, ingenieros de compiladores, desarrolladores de sistemas

---

## 1. El Dilema de la Memoria en Computación Numérica

1. **El Problema del Tracing Garbage Collector (R, Python, Julia):**
   - Los recolectores de basura con pausas detienen la ejecución de hilos arbitrariamente. En simulaciones Monte Carlo o MCMC de $10^7$ iteraciones, las pausas periódicas degradan la latencia y la predictibilidad temporal.
   - Requieren mantener en RAM entre 2x y 3x la memoria realmente necesaria para evitar ciclos continuos de colección (*thrashing*).
   - Impiden empaquetar librerías dinámicas C-ABI limpias sin arrastrar un runtime pesado.

2. **El Problema de la Gestión Manual Rigurosa (C++, Rust):**
   - Aunque Rust ofrece cero coste y seguridad absoluta, la necesidad de lidiar con tiempos de vida explícitos (`'a`) en transformaciones interactivas de datos y rebanado matricial resulta tediosa para el análisis exploratorio de datos.

---

## 2. La Solución Híbrida de GHL: ARC + CoW + Arenas

GHL adopta una arquitectura de memoria determinista de alto rendimiento sin pausas de Tracing GC:

```
+-------------------------------------------------------------------+
|                     GHL DETERMINISTIC MEMORY                      |
+-------------------------------------------------------------------+
| 1. Structural Value Semantics with Copy-on-Write (CoW)            |
|    -> Cloning a 20 GB DataFrame is an O(1) atomic pointer bump    |
|    -> In-place mutation if unique reference (ref_count == 1)     |
+-------------------------------------------------------------------+
| 2. Automatic Reference Counting with Static Elision               |
|    -> Compiler elides redundant inc/dec through alias analysis    |
+-------------------------------------------------------------------+
| 3. Regional Bump Allocators (Arenas) for Iterative Sampling       |
|    -> Ultra-fast bump allocation inside MCMC / Bootstrap loops    |
|    -> O(1) instant memory reclamation at end of iteration scope   |
+-------------------------------------------------------------------+
```

### 2.1. Semántica de Copia en Escritura (Copy-on-Write)
- Las matrices y tablas de datos comparten búferes inmutables subyacentes.
- Cuando una función modifica una columna o submatriz:
  - Si el objeto tiene un único propietario (`ref_count == 1`), la mutación se realiza **in-place** con coste cero de copia.
  - Si el objeto está compartido entre múltiples variables o hilos concurrentes, se clona de forma transparente el fragmento necesario.

### 2.2. Arenas Regionales para Métodos Iterativos (MCMC & Bootstrap)
En algoritmos con bucles intensivos:
```lang
// Regional Arena keeps heap allocations at zero during sampling
arena::scope(|arena| {
    for step in 0..100_000 {
        let proposal = arena.alloc_vector(dim);
        sample_parameters(&mut proposal, &rng);
        accept_or_reject(&proposal, &mut state);
        // Instant bump pointer reset at loop boundary: O(1) deallocation
    }
});
```

---

## 3. Arquitectura del Compilador y Niveles de Ejecución (Tiering)

```
             [ GHL Source Code (.gh / .ghl) ]
                           │
                           ▼
             [ Parser & Abstract Syntax Tree ]
                           │
                           ▼
          [ Bidirectional Typechecker & HIR ]
                           │
            ┌──────────────┴──────────────┐
            ▼                             ▼
    [ Interactive REPL ]          [ Ahead-Of-Time (AOT) ]
    • Fast Tier-1 Cranelift/VM    • LLVM Optimizing Backend
    • Startup < 20 ms             • SIMD Auto-vectorization & LTO
    • Zero TTFX wait time         • Standalone Native Binary (<15 MB)
```

### 3.1. Arranque Inmediato sin TTFX
- **Modo REPL y Scripts Interactivos (`ghl repl` / `ghl run`):** Impulsado por un compilador rápido de nivel 1 (Cranelift o VM de bytecode), responde en menos de 20 milisegundos, eliminando los molestos retrasos de compilación JIT experimentados en Julia.
- **Modo Compilación de Producción (`ghl build --release`):** Utiliza LLVM para aplicar optimizaciones globales (LTO, autovectorización AVX-512/Neon, desenrollado de bucles), generando binarios autónomos y portables.

### 3.2. Binarios Pequeños y Exportación Nativa C-ABI
A diferencia de Julia (cuyos ejecutables compilados con `PackageCompiler.jl` pesan varios cientos de megabytes), GHL compila a **binarios nativos autónomos de 8 a 20 MB**, sin requerir una máquina virtual residente ni el compilador LLVM embebido en tiempo de ejecución.

