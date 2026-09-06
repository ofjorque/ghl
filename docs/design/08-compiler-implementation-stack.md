# RFC 08: Pila Tecnológica del Compilador y Runtime en Rust

- **Estado:** Aprobado
- **Área:** Arquitectura de Implementación / Crates de Rust / Runtime Internals
- **Audiencia:** Desarrolladores del compilador GHL, arquitectos de software

---

## 1. Visión General de la Pila Tecnológica

El compilador y runtime de **GHL** (*Generalized Hypothesis Language*) se implementan en **Rust** aprovechando las crates más avanzadas, rigurosas y de alto rendimiento del ecosistema contemporáneo.

Esta selección técnica garantiza:
1. **Compilación en Rust puro:** Eliminación de dependencias complejas de Fortran o C para álgebra lineal básica gracias a `faer`.
2. **Experiencia de desarrollo (DX) empática y moderna:** Parseo con recuperación de errores (`chumsky`), diagnósticos visuales enriquecidos con Kaomojis (`ariadne`) y shell interactiva avanzada (`reedline`).
3. **Velocidad de ejecución y cero latencia TTFX:** JIT instantáneo con `cranelift` (< 20 ms) para scripting/REPL y compilación de producción.
4. **Capacidades estadísticas de nivel mundial:** DataFrames columnares Arrow (`polars`), distribuciones (`statrs`, `rand_distr`), aleatoriedad determinista (`rand_xoshiro`) y paralelismo multihilo sin GIL (`rayon`).

---

## 2. Mapa Arquitectónico por Capas

```
+-------------------------------------------------------------------------------+
|                             INTERFAZ DE USUARIO Y DX                          |
|  • CLI: clap (derive)               • REPL Shell: reedline                    |
|  • Diagnósticos: ariadne            • Gráficos & Visualización: plotters      |
+-------------------------------------------------------------------------------+
                                      │
                                      ▼
+-------------------------------------------------------------------------------+
|                             FRONTEND DEL COMPILADOR                           |
|  • Tokenización / Lexer: logos      • Parser con recuperación: chumsky        |
|  • Asignación de AST: bumpalo       • Comprobación de tipos: Inferencia GHL   |
+-------------------------------------------------------------------------------+
                                      │
                                      ▼
+-------------------------------------------------------------------------------+
|                       MOTOR DE EJECUCIÓN Y GENERACIÓN                         |
|  • JIT & Codegen: cranelift (codegen, frontend, jit, module)                  |
|  • Paralelismo de CPU: rayon (Work-stealing threadpool)                       |
+-------------------------------------------------------------------------------+
                                      │
                                      ▼
+-------------------------------------------------------------------------------+
|                         RUNTIME ESTADÍSTICO Y MATEMÁTICO                      |
|  • Álgebra Lineal: faer             • Tablas de Datos: polars (Arrow)         |
|  • Distribuciones: statrs / rand_distr  • PRNG: rand + rand_xoshiro           |
+-------------------------------------------------------------------------------+
```

---

## 3. Desglose Detallado de Crates Seleccionadas

### 3.1. Frontend y Diagnósticos
- **`logos`:** Generador de lexers ultrarrápido basado en autómatas finitos deterministas (DFA) en tiempo de compilación. Convierte el código fuente en tokens a velocidades de gigabytes por segundo.
- **`chumsky`:** Parser combinator con soporte de primer orden para **recuperación de errores**. Si el usuario olvida un delimitador o comete un desliz sintáctico, el parser continúa analizando el resto del archivo para emitir múltiples diagnósticos de valor en una sola pasada.
- **`ariadne`:** Motor de formateo de diagnósticos de terminal de alta precisión. Responsable del renderizado de snippets con números de línea, etiquetas multicolor, sugerencias y los Kaomojis característicos de GHL (`(ノ°□°)ノ`, `ฅ(ﾐΦ ﻌ Φﾐ)ฅ`, `(U・ᴥ・U)`).

### 3.2. Gestión de Memoria Interna del Compilador
- **`bumpalo`:** Asignador por arena (bump allocation) de máxima velocidad. Permite construir árboles de sintaxis abstracta (AST), grafos de flujo de control (CFG) y tablas de tipos sin sobrecarga de asignaciones individuales en el heap general, reciclándose de golpe al concluir cada fase. También sirve de base para las arenas MCMC de GHL.

### 3.3. Núcleo Matemático y Estadístico
- **`faer`:** Biblioteca de álgebra lineal de alto rendimiento en Rust puro.
  - Implementa descomposiciones matriciales (LU, QR, Cholesky, SVD, autovalores) y multiplicación matricial multihilada.
  - Compila directamente en Linux x86_64, macOS ARM64 (Apple Silicon) y Windows sin requerir compiladores de Fortran ni enlaces externos a OpenBLAS o MKL.
- **`polars` (o `polars-core` / `arrow2`):** Motor columnar de DataFrames líder a nivel mundial.
  - Almacenamiento continuo compatible con la especificación Apache Arrow.
  - Filtros vectoriales, joins multihilo, agregaciones de alta cardinalidad y optimización de consultas lazy.
- **`statrs` + `rand_distr`:** Implementación formal de funciones estadísticas:
  - Densidades (PDF), distribución acumulada (CDF) y cuantiles inversos para distribuciones continuas y discretas.
  - Funciones matemáticas especiales: Gamma, Digamma, Beta, función de error (Erf).
- **`rand` + `rand_xoshiro`:** Generador de números pseudoaleatorios basado en el algoritmo **Xoshiro256++**. Garantiza reproducibilidad estricta bit a bit entre distintas plataformas y arquitecturas para semillas fijas.

### 3.4. Concurrencia y Paralelismo
- **`rayon`:** Planificador de hilos con robo de trabajo (*work-stealing*). Facilita iteradores paralelos de coste cero (`.par_iter()`), paralelismo en operaciones de matrices y simulación Monte Carlo concurrente sobre memoria compartida sin problemas de contención de GIL.

### 3.5. Generación de Código y JIT
- **`cranelift` (`cranelift-codegen`, `cranelift-frontend`, `cranelift-jit`, `cranelift-module`):**
  - Generador de código máquina nativo de ultra-baja latencia desarrollado por la Bytecode Alliance.
  - Compila funciones en milisegundos para el REPL y el ejecutor de scripts, resolviendo el cuello de botella del *Time To First Execution* (TTFX).

### 3.6. Interfaz de Línea de Comandos (CLI), REPL y Gráficos
- **`clap` (con `derive`):** Parser de línea de comandos para el ejecutable `ghl`, estructurando subcomandos (`ghl run`, `ghl repl`, `ghl check`, `ghl test`, `ghl build`).
- **`reedline`:** Motor de línea de comandos para el REPL interactivo (creado para Nushell). Soporta edición multilínea, historial persistente, atajos de teclado y autocompletado en vivo.
- **`plotters`:** Motor de renderizado visual de datos estadísticos. Permite graficar curvas de densidad, histogramas y dispersión tanto en archivos PNG/SVG como directamente en la terminal interactiva mediante caracteres braille/ANSI.

---

## 4. Estructura de Workspace de Cargo Planificada

Para mantener el código modular, desacoplado y con tiempos de compilación incrementales rápidos, el proyecto se organizará en un Cargo Workspace:

```
crates/
├── ghl-syntax/      # Lexer (logos), parser (chumsky), AST y spans
├── ghl-diagnostics/ # Renderizado de errores con Kaomojis y ariadne (Compute vs Statistical)
├── ghl-types/       # Sistema de tipos, inferencia bidireccional y verificación dimensional
├── ghl-ir/          # Representación intermedia (HIR / MIR)
├── ghl-codegen/     # Generador de código JIT y AOT con Cranelift
├── ghl-runtime/     # Runtime de ejecución: faer, polars, statrs, rand_xoshiro, arenas
└── ghl-cli/         # Binario principal `ghl`: clap, reedline (REPL) y plotters
```

