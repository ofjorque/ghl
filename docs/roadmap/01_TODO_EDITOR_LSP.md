# Roadmap — Editor (Positron / VS Code) + Language Server Protocol

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre las fases originales 1 y 2 (van juntas: la extensión de editor no tiene valor sin el LSP detrás).

---

## Parte A: Extensión de Editor para Positron / VS Code

Ubicación: `editors/vscode/` (mismo repositorio, sincronización atómica).

- [x] **Configuración de Lenguaje (`language-configuration.json`):**
  - [x] Definir comentarios de línea (`//`) y bloque (`/* ... */`) para habilitar atajos `Ctrl + /` y `Shift + Alt + A`.
  - [x] Configurar autocierre y coincidencia de parejas: `()`, `[]`, `{}`, `#[ ... ]` y `""`.
  - [x] Reglas de auto-indentación tras apertura de llaves y delimitadores tabulares (`dataframe [`, `mat [`).
  - [x] Reglas de plegado de código (*code folding*) para bloques `fn`, `struct`, `impl` y comentarios.
- [x] **Gramática TextMate (`syntaxes/ghl.tmLanguage.json`):**
  - [x] Palabras clave y control de flujo: `let`, `mut`, `fn`, `struct`, `trait`, `impl`, `use`, `if`, `else`, `match`, `while`, `for`, `in`, `return`.
  - [x] Literales semánticos Kleene: `NA` y valores con motivo `NA:Reason` (`constant.language.na.ghl`).
  - [x] Operadores pipeline y fórmulas estadísticas: `|>` y `~`.
  - [x] Operadores matriciales y vectorizados: `\`, `.*`, `.+`, `.-`, `./`.
  - [x] Funciones nativas y verbos estándar destacados: `ols`, `fit`, `summary`, `filter`, `select`, `mutate`, `plot`, `mean`, etc.
  - [x] Anotaciones de tipos: `f64`, `i64`, `String`, `Bool`, `Vector[T]`, `Matrix`, `DataFrame`, `ModelFit`.
- [x] **Snippets Ergonómicos (`snippets/ghl.json`):**
  - [x] Plantilla de función `fn`.
  - [x] Plantilla de modelo estadístico `ols(y ~ x, df)`.
  - [x] Plantilla de pipeline de datos `df |> filter(...) |> summarize(...)`.
  - [x] Plantilla de gráfico `plot(...) |> geom_point() |> show()`.
- [x] **Manifiesto y Empaquetado VSIX:**
  - [x] `package.json` con asociación para archivos `.gh` y `.ghl`.
  - [x] Generación del paquete `ghl-0.1.0.vsix` listo para instalar en Positron (*Install from VSIX*).

---

## Parte B: Language Server Protocol (`ghl lsp`)

Creación de crate dedicado o subcomando `ghl lsp` para comunicación estándar JSON-RPC sobre stdio.

- [x] **Infraestructura del Servidor:**
  - [x] Integración de biblioteca LSP (ej. `tower-lsp`).
  - [x] Comando CLI `ghl lsp` para iniciar el servidor de lenguaje.
  - [x] Conexión del cliente en la extensión de VS Code/Positron al binario `ghl`.
- [x] **Diagnósticos en Tiempo Real (as-you-type):**
  - [x] Notificación de errores de sintaxis (`textDocument/publishDiagnostics`) en cada cambio de documento.
  - [x] Subrayado ondulado rojo/amarillo para errores de tipos e inconsistencias de dimensiones matriciales.
- [x] **Hover Documentation:**
  - [x] Respuesta a `textDocument/hover`: renderizar tooltip flotante en Markdown con:
    - Firma de la función.
    - Fórmula matemática en notación limpia (ej. $\hat{\beta} = (X^TX)^{-1}X^Ty$).
    - Descripción de parámetros y ejemplo de uso proveniente de `FunctionDoc`.
- [x] **Autocompletado Contextual:**
  - [x] Autocompletado de funciones de la biblioteca estándar (`mean`, `ols`, `filter`, etc.).
  - [x] Autocompletado de variables locales y funciones declaradas en el archivo actual.
  - [x] Autocompletado inteligente de nombres de columnas en contextos de verbos DataFrames (`filter`, `select`, `mutate`).
- [x] **Navegación:**
  - [x] *Go to Definition* (`textDocument/definition`) para saltar a la declaración de funciones y variables de usuario.
