# Roadmap — Documentación de Usuario, Guías de Migración y Cookbook

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre la fase original 8.

Creación del directorio `docs/guides/` con material didáctico para usuarios finales:

- [ ] **Guía de Migración para Usuarios de R (`docs/guides/ghl_for_r_users.md`):**
  - [ ] Tabla de equivalencias de funciones: `lm()` -> `ols()`, `glm()` -> `fit_logistic()`, `summary()`, `predict()`.
  - [ ] Comparativa Tidyverse vs GHL: `%>%` / `|>` con `filter()`, `mutate()`, `group_by()`.
  - [ ] Manejo de `NA`: del `is.na()` tradicional a los motivos semánticos `NA:Reason` y lógica de Kleene.
- [ ] **Guía de Migración para Usuarios de Python / Pandas (`docs/guides/ghl_for_python_users.md`):**
  - [ ] Comparativa Pandas/Polars vs DataFrames GHL.
  - [ ] Equivalencias de Álgebra Lineal: NumPy/SciPy vs GHL `dot()`, `\`, `cholesky()`, `qr()`.
  - [ ] Sintaxis de expresiones funcionales frente a métodos de objetos.
- [ ] **Tutorial Rápido "GHL en 15 Minutos" (`docs/guides/quickstart.md`):**
  - [ ] Instalación del binario y uso del REPL.
  - [ ] Hola Mundo estadístico: carga de CSV, filtrado, regresión OLS y visualización.
- [ ] **The GHL Statistical Cookbook (`docs/guides/cookbook.md`):**
  - [ ] Receta 1: Limpieza e imputación de datos faltantes con motivos semánticos (`impute`, `filter_na_reason`).
  - [ ] Receta 2: Regresión Lineal Robusta con errores estándar corregidos por heterocedasticidad (HC0 a HC3).
  - [ ] Receta 3: Clasificación Binaria con Regresión Logística (IRLS).
  - [ ] Receta 4: Agrupamiento no supervisado con Modelos de Mezclas Gaussianas (GMM / EM).
  - [ ] Receta 5: Muestreo Bayesiano MCMC y Bootstrap paralelo reproducible.
- [ ] **Generador de Documentación de Código (`ghl doc` — estilo roxygen2 / rustdoc):**
  - [ ] Captura de comentarios de documentación `///` en declaraciones `fn`, `struct`, `trait`.
  - [ ] Soporte de Markdown y tags semánticos (`@param`, `@return`, `@example`, `@formula`).
  - [ ] Generación automática de páginas de ayuda en Markdown/HTML navegables para paquetes y proyectos.
  - [ ] Integración con el LSP para enriquecer el Hover de funciones del usuario y soporte de ayuda en el REPL (`?fun`).

