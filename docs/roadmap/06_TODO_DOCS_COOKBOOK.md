# Roadmap — Documentación de Usuario, Guías de Migración y Cookbook

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre la fase original 8.

Creación del directorio `docs/guides/` con material didáctico para usuarios finales:

- [x] **Guía de Migración para Usuarios de R (`docs/guides/ghl_for_r_users.md`):**
  - [x] Tabla de equivalencias de funciones: `lm()` -> `ols()`, `glm()` -> `fit_logistic()`, `summary()`, `predict()`.
  - [x] Comparativa Tidyverse vs GHL: `%>%` / `|>` con `filter()`, `mutate()`, `group_by()`.
  - [x] Manejo de `NA`: del `is.na()` tradicional a los motivos semánticos `NA:Reason` y lógica de Kleene.
- [x] **Guía de Migración para Usuarios de Python / Pandas (`docs/guides/ghl_for_python_users.md`):**
  - [x] Comparativa Pandas/Polars vs DataFrames GHL.
  - [x] Equivalencias de Álgebra Lineal: NumPy/SciPy vs GHL `dot()`, `\`, `cholesky()`, `qr()`.
  - [x] Sintaxis de expresiones funcionales frente a métodos de objetos.
- [x] **Tutorial Rápido "GHL en 15 Minutos" (`docs/guides/quickstart.md`):**
  - [x] Instalación del binario y uso del REPL.
  - [x] Hola Mundo estadístico: carga de CSV, filtrado, regresión OLS y visualización.
- [x] **The GHL Statistical Cookbook (`docs/guides/cookbook.md`):**
  - [x] Receta 1: Limpieza e imputación de datos faltantes con motivos semánticos (`impute`, `filter_na_reason`).
  - [x] Receta 2: Regresión Lineal Robusta con errores estándar corregidos por heterocedasticidad (HC0 a HC3).
  - [x] Receta 3: Clasificación Binaria con Regresión Logística (IRLS).
  - [x] Receta 4: Agrupamiento no supervisado con Modelos de Mezclas Gaussianas (GMM / EM).
  - [x] Receta 5: Muestreo Bayesiano MCMC y Bootstrap paralelo reproducible.
- [x] **Generador de Documentación de Código (`ghl doc` — estilo roxygen2 / rustdoc):**
  - [x] Captura de comentarios de documentación `///` en declaraciones `fn`, `struct`, `trait`.
  - [x] Soporte de Markdown y tags semánticos (`@param`, `@return`, `@example`, `@formula`).
  - [x] Generación automática de páginas de ayuda en Markdown/HTML navegables para paquetes y proyectos.
  - [x] Integración con el LSP para enriquecer el Hover de funciones del usuario y soporte de ayuda en el REPL (`?fun`).

