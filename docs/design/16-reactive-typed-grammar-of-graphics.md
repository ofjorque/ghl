# RFC 16: Gramática de Gráficos Reactiva, Tipada y Composicional (`ghl_plot`)
## Arquitectura de Visualización Científica Nativa para GHL

- **Versión**: 0.4.0 (Fundada en Tipos Nativos de GHL)
- **Estado**: Aprobado / En Implementación
- **Área**: Visualización Científica / Compilador / Runtime / Interoperabilidad
- **Dependencias**: [RFC 01](01-syntax-and-grammar.md), [RFC 02](02-type-system-and-semantics.md), [RFC 09](09-categorical-data-graphics-and-numerical-validation.md), [RFC 11](11-neko-statistical-modeling-framework.md), [RFC 12](12-cockpit-deck-telemetry-and-diagnostics.md)
- **Tributo**: Leland Wilkinson (*The Grammar of Graphics*), Hadley Wickham (*ggplot2*), y Arvind Satyanarayan (*Vega-Lite*).

---

## Tabla de Contenidos

1. [Resumen Ejecutivo y Visión](#1-resumen-ejecutivo-y-visión)
2. [Diagnóstico Crítico: Deficiencias de la Implementación Previa y Estado del Arte](#2-diagnóstico-crítico-deficiencias-de-la-implementación-previa-y-estado-del-arte)
3. [Principios Rectores de Diseño](#3-principios-rectores-de-diseño)
4. [Arquitectura del Pipeline Gráfico](#4-arquitectura-del-pipeline-gráfico)
5. [Nivel 0 — La Representación Intermedia (IR)](#5-nivel-0--la-representación-intermedia-ir)
6. [Sistema de Tipos: Tipos Nativos de GHL y Escalas Visuales](#6-sistema-de-tipos-tipos-nativos-de-ghl-y-escalas-visuales)
7. [Semántica de Evaluación y Ejecución](#7-semántica-de-evaluación-y-ejecución)
8. [Extensibilidad: Geoms Compuestos y Nodos Estadísticos](#8-extensibilidad-geoms-compuestos-y-nodos-estadísticos)
9. [Ergonomía de Superficie y Capa de Azúcar](#9-ergonomía-de-superficie-y-capa-de-azúcar)
10. [Ejemplos Completos y Flujo de Trabajo en GHL](#10-ejemplos-completos-y-flujo-de-trabajo-en-ghl)
11. [Backends de Renderizado: Arquitectura Dual (Plotters + Vega)](#11-backends-de-renderizado-arquitectura-dual-plotters--vega)
12. [Integración con el Ecosistema GHL (DataFrame, Factores, NEKO y Cockpit)](#12-integración-con-el-ecosistema-ghl-dataframe-factores-neko-y-cockpit)
13. [Riesgos, Mitigaciones y Plan de Implementación](#13-riesgos-mitigaciones-y-plan-de-implementación)

---

## 1. Resumen Ejecutivo y Visión

**Tesis.** Una gráfica estadística es un *grafo de flujo de datos tipado (Dataflow DAG)*. La gramática no es una colección de funciones de dibujo imperativo, sino una **Representación Intermedia (IR)** estructuralmente inmutable, serializable, editable e inspeccionable. La sintaxis de superficie (tuberías `|>`, azúcar de geoms y capas) elabora de forma determinista hacia esta IR.

En GHL, este diseño corrige de raíz los compromisos históricos de `ggplot2` (carentes de tipado estricto, evaluación no estándar impredecible y closures opacos) y de `Vega-Lite` (complejidad arbitraria de reglas de `resolve`, tipos basados en strings sin verificación estática y falta de ergonomía en terminal).

Las seis decisiones estructurales en GHL:

| # | Decisión de Diseño | Qué Resuelve en GHL |
|---|---|---|
| 1 | **La especificación es un dato (`Spec`)** | Permite versionar, comparar (`diff`), inspeccionar (`explain`) y compilar gráficos como valores de primera clase (`Value::Plot`). |
| 2 | **Arquitectura Dual de Backends (`Plotters` + `Vega`)** | Renderizado nativo headless ultraveloz en Rust puro para CLI/PNG/SVG/PDF y emisión de especificaciones interactivas Vega JSON para Positron, Jupyter y notebooks. |
| 3 | **Escalas nombradas de primera clase** | Elimina las reglas de resolución opacas: capas que citan la misma escala comparten automáticamente dominio y producen una sola leyenda coherente. |
| 4 | **Álgebra de vistas unificada (`Layer`, `Beside`, `Below`, `Facet`)** | Permite componer múltiples capas reales en el mismo sistema cartesiano y facetear composiciones completas sin herramientas externas. |
| 5 | **Nodos estadísticos y ajustes de posición en el DAG** | Los estadísticos (`smooth`, `boxplot`) y ajustes (`stack`, `dodge`) producen columnas visibles consultables (`yhat`, `q1`, `med`, `q3`). |
| 6 | **Integración 100% nativa con Tipos de GHL (`F64`, `Factor`, Polars y NEKO)** | Aprovecha directamente el sistema de tipos de GHL: `Type::F64`, `Type::I64`, `Type::Factor`, `Type::String`, `Type::Bool` y fechas, sin clasificaciones externas artificiales. |

---

## 2. Diagnóstico Crítico: Deficiencias de la Implementación Previa y Estado del Arte

### 2.1 Qué estaba roto en el prototipo inicial de GHL (`ghl-diagnostics::plot`)
1. **Pseudo-Gramática Excluyente:** El renderizador utilizaba una cascada condicional (`if is_hist { ... } else if is_box { ... } else { scatter }`), haciendo imposible superponer capas básicas como puntos con una curva de regresión (`geom_point + geom_smooth`) o histogramas con densidad.
2. **`geom_line` Incompleto:** La variante existía en el enum pero el motor de dibujo la ignoraba, graficando siempre puntos cartesianos.
3. **Canales Estéticos Rotos:** Aunque `aes()` aceptaba `color`, el runtime nunca extraía los datos de la columna asociada del DataFrame; los puntos se dibujaban con un color fijo sin mapeo a paletas.
4. **Restricción a `f64` Continuos:** `PlotSpec` solo almacenaba vectores numéricos planos, careciendo de soporte para variables categóricas (`Factor`), cadenas o fechas.
5. **Acoplamiento Indebido:** El motor gráfico residía en `crates/ghl-diagnostics`, mezclando la generación de reportes de error del compilador con dependencias pesadas de dibujo.

### 2.2 Qué está roto en `ggplot2`
- **La especificación no es un dato inspeccionable:** `aes()` utiliza evaluación no estándar (NSE) y las capas son closures opacos.
- **Las escalas son derivadas e implícitas:** Compartir una leyenda entre dos canales o tener múltiples escalas intencionales requiere trucos frágiles.
- **Dualidad confusa `stat`/`geom`:** Tablas intermedias ocultas accesibles solo mediante parches como `after_stat()`.
- **Ausencia de tipos:** Los errores de dimensión o dominio inválido solo saltan al momento de renderizar.

### 2.3 Qué está roto en `Vega-Lite`
- **Tipos por strings débiles:** `"quantitative"`, `"nominal"`, `"ordinal"` en Vega-Lite son strings sueltos sin verificación estática ni integración con tipos estructurados como los Factores de GHL.
- **Reglas de `resolve` ad hoc:** Requiere memorizar tablas de casos para decidir si escalas y leyendas se comparten o se independizan.
- **Falta de soporte de terminal:** Requiere V8, Node.js o un navegador para generar cualquier gráfico.

---

## 3. Principios Rectores de Diseño

- **P1 — La especificación es un dato:** Cualquier gráfico se puede escribir, serializar a JSON, clonar, inspeccionar con `explain(p)` y transformar mediante transformaciones puras (`Spec -> Spec`).
- **P2 — Nada implícito sin nombre:** Escalas, canales, paneles y nodos de transformación estadística tienen identidad y nombres estables deterministas.
- **P3 — Una operación, una etapa:** Filtrar datos (`Filter`), acotar dominio de escala (`Scale.domain`) y recortar ventana visual (`Coord.window`) ocurren en etapas desacopladas y no se confunden.
- **P4 — Distinción clara entre mapeo y constante:** `color: col("grupo")` pasa por una escala entrenada; `color: const("red")` aplica un valor visual directo sin generar categorías espurias en leyendas.
- **P5 — Sin acantilado de abstracción:** El usuario puede empezar con sintaxis concisa de tubería (`df |> plot(...) |> geom_point()`) y sobreescribir aspectos finos en la misma expresión sin tener que reescribir todo a bajo nivel.
- **P6 — El sistema de tipos de GHL previene gráficos engañosos:** Columnas cuantitativas (`F64`, `I64`) en canales de área o barras exigen origen natural (`zero: true`, principio de tinta proporcional) a nivel de chequeo.

---

## 4. Arquitectura del Pipeline Gráfico

El compilador de visualización elabora la sintaxis de superficie hacia una **Representación Intermedia (IR)** desacoplada del motor de salida, bifurcándose en la etapa final de renderizado:

```
                      [ Sintaxis GHL: Tuberías |> o Bloques plot { } ]
                                      │
                                 (elaborate)
                                      │
                                      ▼
                    ┌───────────────────────────────────┐
                    │   IR: Spec (DAG, Scales, View)    │
                    │   + Tipos GHL, Factores & Canales │
                    └─────────────────┬─────────────────┘
                                      │
                       ¿Target Nativo o Interactivo?
                                      │
                    ┌─────────────────┴─────────────────┐
                    ▼                                   ▼
          [ Target: PLOTTERS ]                 [ Target: VEGA ]
       (CLI, PNG, SVG, PDF, Terminal)       (Positron, Jupyter, Web)
                    │                                   │
         1. Evalúa Data DAG en Rust          1. Serializa Data DAG a JSON
            (Polars / Arrow en memoria)          o transforms Vega nativos
         2. Layout Solver en Rust            2. Mapea Signals a eventos
            (geometría px exacta)                interactivos web (brush/zoom)
         3. Renderiza primitivas Rust        3. Aplica Layout Solver fijo
         4. Salida PNG/SVG o Cockpit         4. Emite especificación Vega
            Deck en terminal (Unicode)           (application/vnd.vega.v5+json)
```

---

## 5. Nivel 0 — La Representación Intermedia (IR)

### 5.1 Estructura Central de la `Spec`

```rust
pub struct PlotSpec {
    pub data: BTreeMap<String, DataNode>,
    pub signals: BTreeMap<String, Signal>,
    pub scales: BTreeMap<String, ScaleSpec>,
    pub guides: BTreeMap<String, GuideSpec>,
    pub view: ViewNode,
}
```

### 5.2 Grafo de Transformación de Datos (DAG)
Los datos no están confinados a un `data` por capa; forman un grafo donde las salidas estadísticas intermedias son consultables:

```rust
pub enum DataNode {
    Source { table_ref: String },
    Transform {
        input: String,
        op: DataOp,
    },
}

pub enum DataOp {
    Filter(Expr),
    Derive { col: String, expr: Expr },
    Aggregate { by: Vec<String>, aggs: Vec<(String, AggExpr)> },
    Bin { field: String, bins: usize },
    Density { field: String, kernel: KernelKind, bandwidth: Option<f64> },
    Fit { formula: Formula, model: ModelFamily }, // Integra con NEKO
    Stack { by: Option<String>, order: SortOrder },
    Dodge { by: String, padding: f64 },
}
```

### 5.3 Escalas Nombradas de Primera Clase
Cada escala transforma un dominio de datos en un rango visual. Al poseer identidad con nombre, las capas que comparten escala se fusionan de manera natural:

```rust
pub struct ScaleSpec {
    pub name: String,
    pub input_type: ghl_types::Type, // Type::F64, Type::Factor, Type::String, etc.
    pub channel: VisualChannel,      // X, Y, Color, Fill, Size, Shape, Alpha, LineType
    pub domain: DomainSpec,
    pub range: RangeSpec,
    pub transform: ScaleTransform,   // Linear, Log10, Sqrt, DiscreteBand, Temporal
    pub zero: bool,                  // Exigido true para barras y áreas
    pub nice: bool,
}
```

### 5.4 Vistas: Álgebra de Composición Unificada
`Layer` y `Facet` forman parte de la misma álgebra composicional:

```rust
pub enum ViewNode {
    Mark(MarkSpec),
    Layer(Vec<ViewNode>),                       // a + b (superposición)
    Beside { left: Box<ViewNode>, right: Box<ViewNode>, align_y: bool }, // a | b
    Below  { top: Box<ViewNode>, bottom: Box<ViewNode>, align_x: bool },  // a / b
    Facet  { by: Vec<String>, view: Box<ViewNode>, layout: FacetLayout }, // a * by(...)
}
```

---

## 6. Sistema de Tipos: Tipos Nativos de GHL y Escalas Visuales

El subsistema gráfico no inventa clasificaciones ajenas ni taxonomías intermedias abstractas. Se fundamenta **exclusivamente en los tipos de datos reales de GHL** ([RFC 02](02-type-system-and-semantics.md)):
- `Type::F64` e `Type::I64`: Datos numéricos cuantitativos (continuos o discretos).
- `Type::Factor { levels, ordered, contrast }`: Variables categóricas de primera clase con niveles explícitos y contrastes estadísticos ([RFC 09](09-categorical-data-graphics-and-numerical-validation.md)).
- `Type::String`: Cadenas de texto, identificadores y etiquetas.
- `Type::Bool`: Máscaras de filtrado y banderas lógicas binarias.
- Series de fecha/tiempo de Polars (`Date` / `Datetime`).

### 6.1 Mapeo de Tipos de GHL a Escalas Visuales

| Tipo en GHL (`Type`) | Canal Visual Primario | Tipo de Escala Gráfica | Comportamiento en GHL |
|---|---|---|---|
| `F64` / `I64` | `x`, `y` | `ScaleTransform::Linear`, `Log10`, `Sqrt` | Eje continuo numérico. Si se asocia a barras o áreas (`geom_bar`, `geom_col`, `geom_area`), el chequeador exige `zero: true` (principio de tinta proporcional). |
| `F64` / `I64` | `color`, `fill` | `ScaleTransform::ContinuousGradient` | Gradiente continuo (secuencial como Viridis, o divergente centrado en 0). |
| `F64` / `I64` | `size`, `alpha` | `ScaleTransform::ContinuousSize` | Mapeo continuo proporcional de radio/grosor y opacidad. |
| `Factor` (`ordered: false`) | `x`, `y` | `ScaleTransform::DiscreteBand` | Eje de bandas discretas con el orden de los niveles definidos en el factor. |
| `Factor` (`ordered: true`) | `x`, `y` | `ScaleTransform::DiscreteBand` | Eje de bandas discretas con orden estrictamente respetado según `levels`. |
| `Factor` | `color`, `fill` | `ScaleTransform::CategoricalPalette` | Paleta de colores discreta (Okabe-Ito accesible por defecto, ColorBrewer). |
| `Factor` | `shape`, `linetype` | `ScaleTransform::DiscreteShape` | Símbolos de puntos discretos o patrones de trazo discontinuo. |
| `String` | `x`, `y`, `color` | `ScaleTransform::DiscreteBand` / `CategoricalPalette` | Se infiere automáticamente como categorías discretas por orden de aparición. |
| `Bool` | `x`, `y`, `color` | `ScaleTransform::DiscreteBand` | Eje o leyenda binaria de dos estados (`false`, `true`). |
| `Date` / `Datetime` | `x`, `y` | `ScaleTransform::Temporal` | Eje continuo temporal con formateo inteligente de fechas (días, meses, años). |

### 6.2 Reglas del Verificador Gráfico
Verificadas estáticamente antes de dibujar:
- **`String` o `Factor` en Eje Continuo:** Error `C0301`: las columnas categóricas o de texto requieren una escala de bandas discretas (`DiscreteBand`), no una escala continua lineal.
- **Tinta Proporcional en Barras/Áreas:** Error `C0302`: si el geom representa magnitud por longitud o área (`geom_bar`, `geom_col`, `geom_area`), el eje cuantitativo (`F64`/`I64`) debe incluir el cero (`zero: true`).
- **Saturación Categórica:** Advertencia `W0301`: asignar una columna `String` o `Factor` con más de 12 niveles distintos al canal `color` emite una advertencia de legibilidad recomendando usar faceting (`* by(...)`) o etiquetado directo.

---

## 7. Semántica de Evaluación y Ejecución

### 7.1 Las Tres Operaciones de Recorte Desacopladas
En GHL, filtrar datos, ajustar el dominio de una escala y hacer zoom en la vista son tres operaciones ontológicamente distintas:

| Operación | Etapa | Efecto Semántico |
|---|---|---|
| `df \|> filter(p)` | 1. Datos (DAG) | Recalcula todas las estadísticas (medias, OLS, densidades) sobre el subconjunto. |
| `scale.domain([a, b])` | 2. Escalas | Recorta el rango de mapeo después de computar las estadísticas. |
| `coord_zoom(xlim: [a, b])`| 3. Coordenadas | Zoom geométrico de la cámara; las estadísticas y el dominio visual se mantienen intactos. |

---

## 8. Extensibilidad: Geoms Compuestos y Nodos Estadísticos

Los geoms avanzados en GHL no son cajas negras; son composiciones transparentes sobre la IR. Por ejemplo, `geom_boxplot()` se descompone en:
1. Un nodo `Aggregate` que calcula `q1`, `med`, `q3`, `lo`, `hi`.
2. Un nodo `Join` con `Filter` para aislar outliers.
3. Un `Layer` compuesto por una barra (`rect`), bigotes (`rule`) y puntos de dispersión (`point`).

Las columnas estadísticas producidas (`yhat`, `residual`, `.fitted`) son accesibles para etiquetado directo:

```lang
// Etiquetado directo del valor de la mediana sin cálculos manuales:
df  |> plot(aes(x: grupo, y: valor))
    |> geom_boxplot()
    |> geom_text(aes(y: stat(med), label: format(stat(med), ".2f")))
```

---

## 9. Ergonomía de Superficie y Capa de Azúcar

GHL proporciona tres niveles ergonómicos de uso:

### 9.1 Nivel 2: Tuberías Declarativas (`|>`) — El Estándar GHL
Es la forma idiomática que los científicos de datos usan a diario:

```lang
use std::plot::*;

df  |> filter(col("score") > 0.0)
    |> plot(aes(x: dose, y: response, color: treatment))
    |> geom_point(alpha: 0.8, size: 3.0)
    |> geom_smooth(method: LM, se: true)
    |> labs(title: "Dose-Response Curve", x: "Dose (mg)", y: "Response")
    |> theme_minimal()
```

### 9.2 Nivel 1: Bloque Declarativo con Escalas Explícitas
Para reportes de publicación y control pixel-perfect:

```lang
plot {
    data ventas = read_csv("ventas.csv") |> filter(!is_na(monto));

    scale px  : F64    -> PositionX { nice: true };
    scale py  : F64    -> PositionY { zero: true };
    scale col : Factor -> Color     { palette: "okabe_ito" };

    view = layer [
        geom_line(ventas, { x: mes@px, y: total@py, color: region@col }),
        geom_point(ventas, { x: mes@px, y: total@py, color: region@col })
    ];

    guides {
        px: axis(bottom),
        py: axis(left),
        col: legend(right)
    };
}
```

### 9.3 Operadores de Composición en Vistas Complejas
Para yuxtaponer o facetear gráficos múltiples:

```lang
let p1 = df |> plot(aes(x: x, y: y)) |> geom_point();
let p2 = df |> plot(aes(x: x)) |> geom_histogram();

// Composición horizontal (Beside) y facetado por categoría:
let dashboard = (p1 | p2) * by(categoria);
```

---

## 10. Ejemplos Completos y Flujo de Trabajo en GHL

### 10.1 Análisis de Regresión y Diagnóstico de Residuos (Conexión NEKO + GHL Plot)

```lang
use std::plot::*;
use std::stats::*;

// 1. Ajuste de modelo lineal con NEKO
let fit = lm(consumo ~ peso + potencia, data: autos);

// 2. Extracción de residuos aumentados (.fitted y .residual automáticos)
let diag_df = augment(fit);

// 3. Gráficos de diagnóstico compuestos
let p_res = diag_df 
    |> plot(aes(x: col(".fitted"), y: col(".residual")))
    |> geom_point(alpha: 0.7)
    |> geom_hline(yintercept: 0.0, linetype: "dashed")
    |> geom_smooth(method: LOESS, se: false, color: const("red"))
    |> labs(title: "Residuos vs Ajustados", x: "Valores Ajustados", y: "Residuos");

let p_qq = diag_df
    |> plot(aes(sample: col(".std_residual")))
    |> geom_qq()
    |> geom_qq_line()
    |> labs(title: "Q-Q Normal");

// Panel lado a lado emitido a PNG y visualizado en terminal:
let panel = p_res | p_qq;
panel |> save("output/diagnostico.png")?;
panel |> show(); // Renderiza preview en terminal
```

---

## 11. Backends de Renderizado: Arquitectura Dual (Plotters + Vega)

### 11.1 Backend Nativo (`Target::Native` — Plotters & Terminal)
- **Motor:** Implementado en `crates/ghl-plot` utilizando la biblioteca `plotters` en Rust puro.
- **Ventaja:** Cero dependencias externas de Node.js, V8 o Chromium.
- **Destinos:**
  - Exportación de alta resolución a archivos vectoriales/raster: `.png`, `.svg`, `.pdf`.
  - Preview en terminal interactivo con `CockpitDeck` usando cuadrículas de bloques Unicode (` `, `▂`, ..., `█`) y caracteres Braille.

### 11.2 Backend Reactivo (`Target::ReactiveSpec` — Vega JSON)
- **Motor:** Serializador declarativo hacia Vega Specification v5 (`application/vnd.vega.v5+json`).
- **Ventaja:** Interactividad completa nativa en IDEs modernos (Positron, VSCode Data Viewer, Marimo, Jupyter).
- **Características:**
  - Brushing interactivo en tiempo real entre múltiples paneles vinculados.
  - Tooltips reactivos al posar el cursor sobre puntos o barras.
  - Zoom y pan en el lienzo del navegador sin reejecutar código GHL.

### 11.3 Layout Solver Unificado en Rust
Para evitar inconsistencias entre Plotters y Vega, **el cálculo de dimensiones de paneles y márgenes se ejecuta siempre en Rust**:
- A Plotters se le entregan las coordenadas absolutas en píxeles.
- A Vega se le emiten marcas `group` con `x`, `y`, `width`, `height` fijos precalculados.

---

## 12. Integración con el Ecosistema GHL (DataFrame, Factores, NEKO y Cockpit)

1. **DataFrames y CoW (Polars/Arrow):**
   `ghl_plot` consume `Value::DataFrame` y `Value::LazyFrame` directamente. Los datos no se copian a vectores intermedios de Rust a menos que sea estrictamente necesario para el rasterizado; las columnas se leen como vistas de Series (`as_f64_view()`).
2. **Tratamiento de `NA:reason`:**
   Los valores faltantes no se descartan silenciosamente. Si una fila contiene `NA:SensorDropout`, el gráfico emite un aviso visual en la leyenda o en el pie de página indicando cuántos puntos fueron excluidos y por qué motivo semántico.
3. **Integración con Positron IDE:**
   Al evaluar una expresión de tipo `Plot` en una sesión con la variable `POSITRON_PLOTS_DIR` activa, GHL renderiza automáticamente el SVG o emite el JSON reactivo al directorio temporal del visor de gráficos de Positron.

---

## 13. Riesgos, Mitigaciones y Plan de Implementación

| Riesgo Técnico | Estrategia de Mitigación en GHL |
|---|---|
| Rendimiento de dibujo en datasets grandes (> 100k filas) | Downsampling automático con agregación por densidad (Hexbin / Bin2D) sugerido por el compilador cuando $N > 20.000$. |
| Mantenimiento de dos generadores de backend | La IR y el Layout Solver son 100% compartidos en Rust; los backends son emisores delgados de dibujo o serialización. |
| Acoplamiento con `ghl-diagnostics` | Creación inmediata de `crates/ghl-plot` extrayendo el código de dibujo de `ghl-diagnostics`. |

### Fases del Roadmap para `ghl_plot`

- [x] **Fase 1: Modularización Limpia (`crates/ghl-plot`)**: Mover el subsistema de gráficos fuera de `ghl-diagnostics` hacia un crate independiente con arquitectura de IR limpia.
- [x] **Fase 2: Compositor de Capas Multi-Layer**: Sustituir el `if/else` excluyente por la estructura `ViewNode::Layer(Vec<MarkSpec>)` permitiendo acumular geometrías sobre un mismo plano cartesiano.
- [x] **Fase 3: Mapeo de Estéticas (`aes`) y Paletas**: Conectar canales `color`, `fill`, `size`, `shape` con las series del DataFrame y paletas categóricas (`Okabe-Ito`, `Viridis`).
- [x] **Fase 4: Soporte Nativo de Factores (`Factor`)**: Mapeo directo de `Value::Factor` (con o sin `ordered: true`) a escalas de bandas discretas y boxplots comparativos.
- [x] **Fase 5: Backend Vega Interactivo**: Emisión de especificaciones Vega JSON para visualización reactiva en Positron y notebooks.
- [x] **Fase 6: Faceting y Álgebra de Composición**: Soporte para `facet_wrap` y `facet_grid` bidimensional, compartición de escalas (`fixed`, `free`, `free_x`, `free_y`), cálculo local de estadísticas y operadores de composición (`+`).
- [x] **Fase 7: Composición Patchwork y Tipografía del Sistema**:
  - Álgebra de composición para vistas independientes: `p1 | p2` (horizontal/beside), `p1 / p2` (vertical/stack), composiciones anidadas tipo dashboard `(p1 | p2) / p3`, y verbos de pipeline `beside(p1, p2)` y `stack(p1, p2)`.
  - Desambiguación sintáctica completa con fórmulas econométricas multipartitas (`y ~ x | fe`).
  - Soporte nativo de fuentes del sistema (`theme(font = "Fira Code", style = "minimal")`): emisión directa en SVG vectoriales (sin los dolores de cabeza de `showtext`/`extrafont` en R) y Vega-Lite v5 (`config.font`).
  - Soporte de etiquetas flexibles: `p + labs(title = "...", x = "...", y = "...")` y `p |> labs(...)`.
  - [x] **Fase 7.1 - Raster OS Font Discovery**: Indexación y auto-descubrimiento en directorios del sistema operativo (`C:\Windows\Fonts`, `/Library/Fonts`, `/usr/share/fonts`) para rasterizado pixel-perfect de fuentes TrueType/OpenType en exportaciones PNG locales vía `plotters::style::register_font` con la feature `ab_glyph`.
