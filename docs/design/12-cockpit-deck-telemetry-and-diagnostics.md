# RFC 12 — Cockpit Deck v0.5.0: Sistema Unificado de Telemetría y Diagnóstico Visual

> *"El motor produce hechos semánticos · los componentes resuelven estructura visual pura · el layout asigna geometría con clipping estricto · los backends proyectan según capacidades."*

- **Versión**: 0.5.0 (Integrada en GHL v0.1.0)
- **Estado**: Activo / Implementado
- **Crates asociados**: `ghl-diagnostics`, `ghl-cli`, `ghl-runtime`

---

## 1. Motivación y Filosofía de Diseño

GHL no concibe la salida de consola como un vertedero de cadenas de texto sin estructura. En la computación estadística moderna, los científicos de datos e investigadores requieren:
1. **Comprensión instantánea de la salud del cálculo**: métricas de convergencia, bondad de ajuste, residuos y colinealidad deben saltar a la vista con claridad visual.
2. **Empatía diagnóstica**: un error no es un castigo, sino una guía. La personalidad dual de GHL (Gojo para computación, Haru/NEKO para estadística) comunica los fallos con calidez.
3. **Resiliencia ante el entorno de ejecución**: ya sea en un emulador de terminal moderno con fuentes ligadas y color verdadero de 24 bits, o en una tubería de integración continua (CI) redirigida a un archivo de texto plano (`NO_COLOR=1`, `TERM=dumb`), la salida debe conservar su legibilidad sin desbordar ni inyectar caracteres de control basura.

Cockpit Deck establece una separación arquitectónica estricta en cuatro niveles:
```text
┌─────────────────────────────────────────────────────────────┐
│  1. Hechos Semánticos (Compiler & NEKO Engine)              │
│     Diagnostics, ModelFit, Residuals, Convergence States    │
├─────────────────────────────────────────────────────────────┤
│  2. Estructura Visual Pura (CockpitDeck Components)         │
│     CockpitPanel, Sparkline, SymbolRegistry, ProgressBar    │
├─────────────────────────────────────────────────────────────┤
│  3. Layout y Geometría con Clipping Estricto                │
│     Truncamiento a COLUMNS, padding alineado, bordes        │
├─────────────────────────────────────────────────────────────┤
│  4. Proyección por Capacidades (RenderCaps)                 │
│     ANSI 24-bit / 256 / 16 color vs NO_COLOR                │
│     Unicode Box-Drawing vs ASCII Puro (+ - |)               │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Matriz de Capacidades de Terminal (`RenderCaps`)

El subsistema `RenderCaps` inspecciona dinámicamente las variables de entorno y los descriptores de archivo del sistema operativo:

| Capacidad | Detección / Variables | Comportamiento Rico | Comportamiento Degradado (Fallback) |
| :--- | :--- | :--- | :--- |
| **Color** | `NO_COLOR`, `FORCE_COLOR`, `TERM=dumb` | Colores ANSI (Rojo error, Verde éxito, Amarillo advertencia, Cian info) | Texto plano monocromático, sin secuencias `\x1b[` |
| **Unicode** | `LANG`, `LC_ALL`, `GHL_ASCII_ONLY` | Bordes redondeados (`╭ ╮ ╰ ╯ │ ─`), Kaomojis (`ฅ(ﾐΦ ﻌ Φﾐ)ฅ`), micro-barras (` ▂▃▄▅▆▇█`) | Bordes ASCII (`+ - |`), badges textuales (`[STAT-ERR]`), barras ASCII (`.:-=+*#@`) |
| **Interactividad** | `libc::isatty(1)`, `GHL_FORCE_TTY` | Telemetría de pipeline interactiva (`[1/3 PARSE] ✔ → [2/3 TYPECHECK] ✔`) | Modo log silencioso y limpio |
| **Geometría** | `COLUMNS` / tamaño ioctl | Adaptación al ancho del terminal (mínimo 40 columnas) | Ancho por defecto de 80 columnas |

---

## 3. Catálogo Semántico de Símbolos (`SymbolRegistry`)

Cockpit Deck estandariza todos los eventos del compilador y motor estadístico mediante códigos canónicos con soporte dual Unicode y ASCII:

### 3.1. Computación y Compilador (Gojo)
- `COMP_SYNTAX`: `(ノಠ益ಠ)ノ彡┻━┻` ── Fallback: `[SYNTAX-ERR]` (Error sintáctico)
- `COMP_TYPE`: `(⊙_☉)` ── Fallback: `[TYPE-ERR]` (Incompatibilidad o desajuste de tipos)
- `COMP_RUNTIME`: `(ノ°□°)ノ` ── Fallback: `[RUNTIME-ERR]` (Fallo de ejecución o desborde)

### 3.2. Estadística e Inferencia (NEKO / Haru)
- `STAT_SINGULAR`: `[| | 0] ฅ(ﾐΦ ﻌ Φﾐ)ฅ` ── Fallback: `[SINGULAR]` (Matriz $X^T X$ singular o deficiente en rango)
- `STAT_COLLINEAR`: `( ;¬_¬) VIF>10` ── Fallback: `[VIF>10]` (Factor de inflación de varianza inflado)
- `STAT_DROPOUT`: `(=ｘェｘ=) NA` ── Fallback: `[NA-DROP]` (Observación descartada por ausencia con causa)
- `STAT_SAMPLE_SIZE`: `(ФωФ) N<5` ── Fallback: `[WARN-N]` (Muestra demasiado pequeña para inferencia robusta)

### 3.3. Ciclo de Vida y Pipeline
- `EXEC_SUCCESS`: `(U・ᴥ・U) ✧` ── Fallback: `[SUCCESS]`
- `EXEC_RUNNING`: `(ง'̀-'́)ง ▶` ── Fallback: `[RUN]`
- `PIPELINE_STEP`: `✔` ── Fallback: `OK`

---

## 4. Componentes Visuales

### 4.1. Tarjetas de Panel (`CockpitPanel`)
Contenedores estructurados con encabezado, insignias de estado (*badges*), pares clave-valor y divisores horizontales.
Garantizan clipping estricto: ninguna línea supera el ancho disponible en el terminal, evitando saltos erráticos.

**Modo Unicode Rico**:
```text
╭─ NEKO Model Fit ─────────────────────────────────────────── (U・ᴥ・U) CONVERGED ─╮
│ Formula: response ~ sensor + id                                              │
│ Observations: 5 valid (1 dropped due to NA)                                  │
│ Residuals: █ ▃▆▆  (min: -0.853, max: +0.801)                                 │
│ Goodness of Fit: R² = 0.9990 | Adj R² = 0.9980 | F = 984.62                  │
│ Criteria: AIC = 21.18 | BIC = 19.62 | Res SE = 0.9038 on 2 DF                │
├──────────────────────────────────────────────────────────────────────────────┤
│ Term               Estimate    Std.Err    t-stat     p-val  Signif           │
│ ---------------- ---------- ---------- --------- ---------  ------           │
│ (Intercept)       -311.1060    86.3668     -3.60    0.0016    **             │
│ sensor               3.1629     0.8776      3.60    0.0016    **             │
│ id                   0.7159     2.4853      0.29    0.8010                   │
╰──────────────────────────────────────────────────────────────────────────────╯
```

**Modo ASCII Puro (`GHL_ASCII_ONLY=1`)**:
```text
+- GHL Verification Deck ----------------------------------------------- [PASS] -+
| Target File: examples/sample.gh                                              |
| Syntax: (=^･ω･^=) Gojo verified syntax (Parsed 22 top-level statements)      |
| Type Safety: (U・ᴥ・U) Haru verified zero semantic/type errors                 |
+------------------------------------------------------------------------------+
```

### 4.2. Sparklines de Residuos (`Sparkline`)
Convierte una serie numérica en una micro-gráfica inline de 8 alturas:
- **Unicode**: `[' ', '▂', '▃', '▄', '▅', '▆', '▇', '█']`
- **ASCII**: `['.', ':', '-', '=', '+', '*', '#', '@']`
- **Downsampling**: Si la serie contiene cientos o miles de observaciones (p. ej., $N = 50,000$), se subdivide en $k$ cubos equi-espaciados promediando sus valores para ajustar la gráfica exactamente al ancho asignado sin deformar la tarjeta.

---

## 5. Puntos de Integración en el Lenguaje

1. **`ghl run`**:
   Muestra el avance de fases de compilación y ejecución (`[1/3 PARSE] ✔ → [2/3 TYPECHECK] ✔ → [3/3 EXECUTE] ▶`) y captura diagnósticos en paneles visuales legibles.
2. **`ghl check`**:
   Genera una tarjeta `GHL Verification Deck` con el resumen del análisis estático y semántico.
3. **`summary(model)` (NEKO)**:
   El método `render_cockpit` se invoca automáticamente al imprimir modelos estadísticos, mostrando parámetros, significancia y sparkline de residuos en un solo bloque autocontenido.
4. **`ghl repl`**:
   Presenta el banner Cockpit Deck interactivo con la versión del runtime y las capacidades activas del entorno.
