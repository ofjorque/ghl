# AGENT.md: Directrices para Agentes de Inteligencia Artificial en GHL

Este repositorio alberga el diseño e implementación de **GHL** (*Generalized Hypothesis Language*, conocido cariñosamente como *Gojo & Haru Language*), un **lenguaje de programación generalista** (no un DSL), compilado y de alto rendimiento, con **semántica de primer orden para computación estadística**, ciencia de datos y modelado probabilístico.

Este documento establece las directrices no negociables, estándares técnicos y principios de diseño que **todo agente autónomo o asistente de IA** debe respetar rigurosamente al trabajar en esta base de código.

---

## 1. Identidad y Filosofía de GHL

1. **Nombre, Extensiones y Herramientas:**
   - **Nombre Oficial:** **GHL** (*Generalized Hypothesis Language*).
   - **Tributo Central:** Nombrado en honor a **Gojo** y **Haru**, dos gatos entrañables. El lenguaje mantiene una atmósfera cálida, empática y amigable con las mascotas (perros y gatos), sin sacrificar un solo miligramo de rigor matemático o informático.
   - **Extensiones de archivo:** `.gh` (predilecta) y `.ghl`.
   - **Binario CLI:** `ghl`.

2. **Idioma y Localización:**
   - **Código y Compilador:** Todo el código fuente del lenguaje (sintaxis, palabras clave, nombres de funciones en la biblioteca estándar, comentarios dentro del código, mensajes de error y diagnósticos) debe estar estrictamente en **inglés**.
   - **Documentación del Proyecto:** Los documentos explicativos y RFCs en `docs/` se redactan en **español** para el equipo de desarrollo.

3. **Taxonomía Dual de Errores (Computación vs. Estadística):**
   - El compilador y el runtime deben clasificar inequívocamente la naturaleza de los problemas:
     - **`[Compute Error]` (`[Cxxxx]`):** Errores de sintaxis, discrepancias de tipos en memoria, punteros nulos no gestionados, desbordamiento de búfer, errores de I/O, fallos de red.
     - **`[Statistical Error]` (`[Sxxxx]`):** Matrices singulares no invertibles, matrices de covarianza que no son semidefinidas positivas, problemas de identificabilidad o multicolinealidad severa, divergencias en cadenas MCMC, violaciones de soporte en distribuciones (ej. evaluar log-densidad fuera del dominio), fallos de convergencia de optimizadores.
     - **`[Statistical Warning]` (`[SWxxxx]`):** Valores inflados de varianza (VIF alto), tamaño de muestra subóptimo para aproximaciones asintóticas ($N < 30$), pérdida de precisión numérica por cancelación catastrófica en punto flotante.

4. **UX con Kaomojis y Empatía por el Usuario:**
   - Los mensajes de diagnóstico deben incorporar Kaomojis expresivos y referencias felinas/caninas para desdramatizar los errores, guiando al usuario con calidez:
     - Compute Errors: Expresiones de sorpresa o tropiezo (`(ノ°□°)ノ`, `(╯°□°)╯︵ ┻━┻`, `(｡•́︿•̀｡)`).
     - Statistical Errors: Ojos atentos felinos o analíticos (`ฅ(ﾐΦ ﻌ Φﾐ)ฅ`, `(・`ω´・)`, `(=^･ω･^=)`).
     - Success / Hints: La lealtad canina y agilidad de Haru (`(U・ᴥ・U) Haru fetched the results!`).

5. **Resolución de los Dos Lenguajes sin Tracing GC:**
   - El código escrito por el usuario en GHL se compila a código nativo de nivel de sistemas (LLVM / Cranelift).
   - Sin pausas de Tracing GC: gestión de memoria mediante ARC optimizado, Copy-on-Write (CoW) con mutación *in-place* garantizada cuando `ref_count == 1`, y arenas de memoria para cálculos estadísticos intensivos.

---

## 2. Flujo de Trabajo y Reglas para Agentes

1. **Consulta de RFCs antes de Modificar Código:**
   - Todo cambio semántico o estructural debe estar respaldado por un documento en `docs/design/`.
2. **Coherencia Léxica en Ejemplos de Código:**
   - Nunca generes ejemplos de código en GHL con identificadores o comentarios en español. Mantén el código 100% en inglés:
     ```lang
     // Good:
     let samples = normal.sample_n(1_000, &mut rng);
     let mean_val = samples.mean();

     // Bad:
     let muestras = normal.sample_n(1_000, &mut rng); // No usar español en código
     ```
3. **Formateo de Diagnósticos:**
   - Asegúrate de que cualquier salida de compilador simulada o generada siga el estándar de Kaomojis y prefijos `[Cxxxx]` o `[Sxxxx]`.
4. **Respeto Estricto al Inventario Canónico de Tokens y Verbos:**
   - Consulta rigurosamente la sección 7 de `docs/design/01-syntax-and-grammar.md`. Ningún agente debe inventar palabras clave, operadores o verbos alternativos fuera del léxico normativo (ej. usar `filter`, `select`, `mutate`, `group_by`, `summarize`, `impute`, `fit`, `predict`, etc.).
5. **Tratamiento de NAs con Motivo Opcional (`NA` y `NA:Reason`):**
   - Todo dato faltante es fundamentalmente `NA`. La especificación de un motivo (`NA:NotObservable`, `NA:NoResponse`, etc.) es **100% opcional** y no intrusiva. Para cualquier función del sistema, un `NA` con motivo sigue siendo un `NA` con lógica ternaria de Kleene.

### 2.1 Principios Operativos y Reglas de Oro de Implementación

Al implementar o modificar código en el compilador o runtime de GHL, el agente debe seguir obligatoriamente estas directrices:

1. **No a la sobreingeniería:** Mantener las soluciones directas, comprensibles y sin capas innecesarias de abstracción.
2. **Reutilizar funciones existentes en lo posible:** Antes de concebir una nueva abstracción o función interna, apalancarse en la infraestructura y funciones ya presentes.
3. **Pensar bien antes de eliminar cualquier función:** No descartar ni alterar interfaces públicas o internas sin justificación estricta.
4. **Pruebas de paridad obligatorias:** Si una función o flujo debe ser modificado, escribir pruebas de regresión y paridad comparando explícitamente el comportamiento original contra el nuevo para asegurar 100% de compatibilidad.
5. **Lenguaje generalista, no un DSL estadístico:** GHL busca ser un lenguaje de programación de propósito general para ciencia y estadística, no un DSL cerrado. Las operaciones deben modelarse como primitivas generales de colecciones, álgebra lineal o control de flujo cuando corresponda, evitando crear palabras clave o primitivas hiperespecíficas para un solo algoritmo.
6. **Compilación y ejecución de pruebas con `--jobs 4`:** Al correr `cargo test`, `cargo build` o `cargo run`, utilizar `--jobs 4` para optimizar el uso de núcleos del procesador.

---

## 3. Estructura Documental y RFCs (`docs/design/`)

- `00-vision-and-philosophy.md`: Visión general, el por qué de GHL y los 5 principios rectores.
- `01-syntax-and-grammar.md`: Gramática formal, operadores `|>` y `~`, macros `pounce!` y `purr!`.
- `02-type-system-and-semantics.md`: Inferencia estática, lógica Kleene de `NA` y tipos dimensionales.
- `03-memory-and-execution-model.md`: ARC + CoW + Arenas de iteración sin Tracing GC; compilación AOT + JIT.
- `04-standard-library-and-primitives.md`: Módulos `core`, `linalg`, `dataframe`, `stats` y `autodiff`.
- `05-concurrency-and-parallelism.md`: Multi-threading sin GIL, SIMD y aceleración en GPU.
- `06-interoperability-and-ecosystem.md`: FFI Zero-Copy con Apache Arrow, puentes Python/R y CLI `ghl`.
- `07-tooling-and-dx.md`: Diagnósticos con Kaomojis, bifurcación `[Compute]` vs `[Statistical]`, REPL y LSP.
- `08-compiler-implementation-stack.md`: Pila de crates en Rust (`chumsky`, `ariadne`, `cranelift`, `faer`, `polars`, `bumpalo`, `statrs`, `rayon`, `reedline`, `clap`, `plotters`).
- `09-categorical-data-graphics-and-numerical-validation.md`: Factores categóricos con contrastes, gramática de gráficos `std::plot` y certificación NIST StRD.
- `10-performance-budgets-and-ergonomic-guarantees.md`: Presupuestos innegociables de velocidad (<15ms), memoria (<10MB), tamaño (<15MB) y UX SLAs.
- `11-neko-statistical-modeling-framework.md`: Arquitectura de modelado estadístico NEKO (desacoplamiento en 3 momentos, Blueprint, capacidades, RowDisposition y covarianzas robustas).
- `12-cockpit-deck-telemetry-and-diagnostics.md`: Sistema unificado de telemetría y diagnóstico visual Cockpit Deck v0.5.0 (RenderCaps, SymbolRegistry, CockpitPanel y Sparklines).
- `13-cranelift-jit-native-compilation.md`: Motor de compilación JIT nativa con Cranelift 0.135 (lowering HIR, SSA, control de flujo nativo y ejecución instantánea sub-milisegundo).
