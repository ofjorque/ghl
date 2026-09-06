# GHL: Generalized Hypothesis Language (`.gh` / `.ghl`)

> **GHL** (*Generalized Hypothesis Language* — secretamente inspirado en los gatos **Gojo & Haru**) es un lenguaje de programación generalista, compilado a código nativo de alto rendimiento, con semántica de primer nivel para computación estadística, análisis de datos y modelado probabilístico.

```
       /\_/\      (U・ᴥ・U)
      ( o.o )       GHL: High-Performance Statistical Computing
       > ^ <        with System-Level Speed and Warm Empathy
```

---

## ¿Por qué GHL?

La ciencia de datos moderna sufre de una fractura constante:
- **R y Python** tienen interfaces expresivas, pero son lentos en bucles nativos y dependen de envoltorios en C/C++/Fortran/Rust.
- **Julia** resolvió los dos lenguajes pero introdujo tiempos de espera en frío (*Time to First Execution* / TTFX), pausas no deterministas por su recolector de basura (Tracing GC) y dificultad extrema para generar binarios pequeños.
- **Rust, Go y C++** son rápidos y confiables, pero carecen de notación matricial ergonómica, semántica formal para datos ausentes (`NA`) y operadores de modelado.

**GHL** reúne lo mejor de ambos mundos:
1. **Velocidad de Sistemas sin Tracing GC:** Compilación AOT vía LLVM con gestión de memoria basada en conteo de referencias atómico (ARC), Copy-on-Write (CoW) con mutación *in-place* y arenas para cómputo iterativo.
2. **Semántica Estadística Nativa:** Matrices, tensores, DataFrames columnares (compatibles con Apache Arrow), operadores de tubería (`|>`), fórmulas de regresión (`~`) y lógica ternaria de Kleene para `NA`.
3. **Diagnósticos Claros con Personalidad:** Diferenciación estricta entre errores computacionales (`[Compute Error]`) y estadísticos (`[Statistical Error]`), aderezados con Kaomojis expresivos (`(=^･ω･^=)`, `(U・ᴥ・U)`) para una experiencia de desarrollo empática y productiva.
4. **Generalista por Derecho Propio:** Capaz de compilar servidores web asíncronos, utilitarios de línea de comandos ultrarrápidos (< 15 MB) o pipelines de inferencia MCMC con el mismo compilador.

---

## Ejemplo Rápido de Código GHL (`analysis.gh`)

```lang
use std::dataframe::DataFrame;
use std::linalg::Matrix;
use std::stats::distributions::Normal;
use std::stats::models::linear_model;

fn main() -> Result<(), SystemError> {
    // Load dataset as a native Apache Arrow columnar DataFrame
    let df = DataFrame::read_parquet("data/trials.parquet")?
        .filter(col("dose") > 0.0 && !is_na(col("response")))
        .mutate(log_dose: log(col("dose")));

    // First-class formula syntax for regression modeling
    let formula = response ~ log_dose + age + (1 | clinic_id);
    let fit = linear_model(formula, data: &df)?;

    // Empathetic, structured summary output
    println("(=^･ω･^=) Model successfully fitted in 4.2ms!");
    fit.summary();

    Ok(())
}
```

---

## Estructura del Repositorio

- [**`AGENT.md`**](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/AGENT.md): Instrucciones operativas y taxonomía de errores para colaboradores de IA.
- [**`docs/design/`**](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/): Documentos de diseño técnico y especificaciones modulares:
  - [`00-vision-and-philosophy.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/00-vision-and-philosophy.md): Visión de GHL, principios rectores y balance entre rigor y calidez.
  - [`01-syntax-and-grammar.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/01-syntax-and-grammar.md): Gramática formal, operadores `|>` y `~`, indexación base 0 con slices.
  - [`02-type-system-and-semantics.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/02-type-system-and-semantics.md): Inferencia fuerte, semántica trivalente de `NA` y tipos dimensionales.
  - [`03-memory-and-execution-model.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/03-memory-and-execution-model.md): Compilación AOT/JIT, ARC, CoW y arenas de iteración.
  - [`04-standard-library-and-primitives.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/04-standard-library-and-primitives.md): Módulos `core`, `linalg`, `dataframe`, `stats` y `autodiff`.
  - [`05-concurrency-and-parallelism.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/05-concurrency-and-parallelism.md): SIMD, multi-threading sin GIL, DataFrames paralelos y GPU.
  - [`06-interoperability-and-ecosystem.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/06-interoperability-and-ecosystem.md): Compatibilidad Zero-Copy con Apache Arrow, C-ABI y conectores Python/R.
  - [`07-tooling-and-dx.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/07-tooling-and-dx.md): Sistema de diagnósticos con Kaomojis, bifurcación `[Compute]` vs `[Statistical]`, REPL y LSP.
  - [`08-compiler-implementation-stack.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/08-compiler-implementation-stack.md): Pila tecnológica de crates en Rust (`faer`, `chumsky`, `ariadne`, `cranelift`, `polars`, etc.).
  - [`09-categorical-data-graphics-and-numerical-validation.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/09-categorical-data-graphics-and-numerical-validation.md): Factores categóricos con contrastes, gramática de gráficos (`std::plot`) y certificación NIST StRD.
  - [`10-performance-budgets-and-ergonomic-guarantees.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/design/10-performance-budgets-and-ergonomic-guarantees.md): Presupuestos cuantitativos de velocidad, memoria, tamaño y contratos de UX.
- [**`benchmarks/`**](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/):
  - [`methodology.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/methodology.md): Protocolo experimental reproducible.
  - [`comparisons/`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/comparisons/): Comparativas exhaustivas vs R, Python y Julia.
  - [`suites/`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/benchmarks/suites/): Suites reproducibles (álgebra lineal, DataFrames, MCMC, startup TTFX).
