# RFC 13 — Motor de Compilación JIT Nativa con Cranelift en GHL

> *"El código estadístico debe ser expresivo al modelar, pero despiadadamente veloz al calcular: cero sobrecoste de intérprete en bucles numéricos y funciones analíticas."*

- **Versión**: 0.1.0 (Integrada en GHL v0.1.0)
- **Estado**: Activo / Implementado
- **Crates asociados**: `ghl-ir`, `ghl-codegen`, `ghl-cli`, `ghl-runtime`
- **Backend**: Cranelift 0.135 (Bytecode Alliance / Wasmtime)

---

## 1. Motivación y Arquitectura

En la computación estadística y científica, la ejecución puramente interpretada introduce un sobrecoste inaceptable en bucles numéricos cerrados, algoritmos recursivos y simulaciones MCMC. Mientras que lenguajes como Julia introdujeron tiempos de espera en frío intolerables (*Time to First Execution* / TTFX) y recolectores de basura no deterministas, GHL adopta **Cranelift** como backend JIT:
1. **Arranque Instantáneo (Sub-milisegundo)**: Cranelift compila a código de máquina x86_64 / aarch64 en fracciones de milisegundo, permitiendo un flujo interactivo en el REPL y scripts sin pausas perceptibles.
2. **Sin Tracing GC**: Toda la memoria sigue gobernada por la gestión determinista de GHL (ARC, CoW y Arenas).
3. **Estrategia Híbrida Inteligente**:
   - **Funciones Numéricas Escalares**: Se bajan a HIR y se compilan directamente a instrucciones nativas de CPU.
   - **Operaciones de Macro-Datos (DataFrames, Matrices, NEKO OLS)**: Se despachan a las rutinas vectorizadas multihilo del runtime en C/Rust.

```text
Código Fuente GHL (.gh)
          │  (ghl-syntax)
          ▼
    AST Verificado
          │  (ghl-types: Inferencia & Recursión)
          ▼
  GHL High-level IR (HIR)  <── [ghl-ir]
          │  (ghl-codegen: FunctionCompiler)
          ▼
    Cranelift IR (CLIF)
          │  (JITBuilder::with_isa)
          ▼
Código Máquina Nativo en Memoria Ejecutable (x86_64)
```

---

## 2. Representación Intermedia (`ghl-ir`)

El crate `ghl-ir` formaliza un nivel intermedio fuertemente tipado que desacopla la sintaxis del lenguaje respecto del backend de generación de código:

### 2.1. Tipos Escalares
- `HirType::I64`: Enteros signados de 64 bits (`cranelift::types::I64`).
- `HirType::F64`: Coma flotante de precisión doble (`cranelift::types::F64`).
- `HirType::Bool`: Booleanos de 1 byte (`cranelift::types::I8`).
- `HirType::Unit`: Valor nulo/vacío (`()`).

### 2.2. Expresiones y Sentencias
- `HirExpr`: Literales, variables locales, operaciones binarias (`HirBinaryOp`), unarias (`HirUnaryOp`), llamadas a función directa (`HirExpr::Call`), condicionales (`HirExpr::IfElse`) y bloques con ámbito (`HirExpr::Block`).
- `HirStatement`: Declaración inmutable (`HirStatement::Let`), reasignación (`HirStatement::Assign`), expresiones de efecto y retornos explícitos.
- `HirFunction`: Prototipo canónico con firma tipada de parámetros y cuerpo lowered.

---

## 3. Emisión de Código y Memoria Ejecutable (`ghl-codegen`)

El crate `ghl-codegen` gestiona la traducción de HIR a Cranelift IR y la asignación de memoria ejecutable (`RX`):

### 3.1. Asignación de Variables SSA
Cranelift opera estrictamente en Forma de Asignación Única Estática (SSA). Para simplificar la generación sin requerir cálculo manual de funciones $\phi$:
- Cada variable o parámetro de GHL se declara mediante `FunctionBuilder::declare_var(clif_ty)`.
- Las lecturas utilizan `FunctionBuilder::use_var(var)` y las escrituras `FunctionBuilder::def_var(var, val)`.
- El algoritmo de Cranelift optimiza las variables locales convirtiéndolas en registros de hardware o argumentos de bloque SSA de manera óptima.

### 3.2. Control de Flujo y Bifurcaciones
Las estructuras condicionales `if / else` se traducen creando 3 bloques básicos:
```text
      [Bloque Origen]
             │
      ins().brif(cond)
      ┌──────┴──────┐
      ▼             ▼
[Then Block]   [Else Block]
      │             │
      └──────┬──────┘
             ▼
       [Merge Block]
```
Una variable temporal de resultado acumula el valor producido por la rama tomada, permitiendo que `if / else` actúe como una expresión tipada.

### 3.3. Invocación Nativa
Una vez emitidas las funciones, `module.finalize_definitions()` bloquea las páginas de memoria con permisos de ejecución y expone punteros de función C-ABI:
```rust
let fib_fn: extern "C" fn(i64) -> i64 = jit.get_fn_i64_1("fib").unwrap();
let result = fib_fn(10); // Ejecución nativa pura a velocidad de CPU
```

---

## 4. Telemetría Cockpit Deck en Tiempo Real

El compilador de línea de comandos `ghl` reporta la actividad de Cranelift JIT en vivo:
```text
╭─ GHL Verification Deck ───────────────────────────────────────── (U・ᴥ・U) PASS ─╮
│ Target File: examples/jit_fibonacci.gh                                       │
│ Syntax: (=^･ω･^=) Gojo verified syntax (Parsed 4 top-level statements)       │
│ Type Safety: (U・ᴥ・U) Haru verified zero semantic/type errors                 │
│ Cranelift JIT: Verified 2 functions ready for native machine code            │
╰──────────────────────────────────────────────────────────────────────────────╯
```
Y durante la ejecución interactiva:
```text
[1/3 PARSE] ✔  [2/3 TYPECHECK] ✔  [3/3 CRANELIFT JIT ▶ (2 functions compiled in 0.85ms)]
```
