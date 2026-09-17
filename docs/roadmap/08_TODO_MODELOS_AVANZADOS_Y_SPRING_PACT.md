# Roadmap 08 — Modelos Econométricos Avanzados, SEM y Librería `spring_pact`

> Backlog temático activo derivado de [`TODO.md`](../../TODO.md) y [`RFC 14`](../design/14-statistical-contracts-and-expectation-protocols.md). 
> Aborda la expansión del motor econométrico NEKO y la concepción de **`spring_pact`**, la primera librería oficial desarrollada 100% en código fuente GHL.

---

## Parte A: Fórmulas y Estimadores de Ecuaciones Estructurales (SEM / CFA)

Cierre de la brecha analítica para constructos latentes y modelos de ecuaciones estructurales:

- [ ] **Sintaxis de Fórmulas SEM en `ghl-syntax`:**
  - [ ] Reconocimiento léxico de nuevos operadores:
    - `=~` (`Token::EqualTilde`): Especificación de variables latentes (*"is measured by"* / modelo de medición).
    - `~~` (`Token::TildeTilde`): Covarianzas residuales y varianzas libres (*"correlated with"*).
  - [ ] Extensión del parser de fórmulas para permitir especificaciones multi-ecuación (bloques de fórmulas SEM).
  - [ ] Soporte de formateo canónico en `ghl fmt` e inspección en LSP.
- [ ] **Estimador SEM en el Runtime (`ghl-runtime::neko`):**
  - [ ] Cálculo de matriz de covarianzas empírica observada $S$.
  - [ ] Optimización de función de discrepancia de Máxima Verosimilitud de Wishart ($F_{ML} = \log|\Sigma(\theta)| + \text{tr}(S \Sigma^{-1}(\theta)) - \log|S| - p$).
  - [ ] Extracción de índices de ajuste global canónicos: $\chi^2$, CFI (*Comparative Fit Index*), TLI, RMSEA y SRMR.
  - [ ] Integración con salidas estandarizadas: `summary()`, `tidy()` y panel Cockpit Deck.

---

## Parte B: Estimadores Econométricos Canónicos

Implementación de algoritmos estándar de la práctica empírica:

- [ ] **Efectos Fijos de Alta Dimensión (Panel Data — `feols`):**
  - [ ] Sintaxis de absorción con operador de proyección `|` (`y ~ x | entity + time`).
  - [ ] Algoritmo de centrado por medias (*Within-transformation* / Frisch-Waugh-Lovell).
  - [ ] Errores estándar clusterizados por grupo (*clustered standard errors*).
- [ ] **Variables Instrumentales (IV / 2SLS — Two-Stage Least Squares):**
  - [ ] Estimador `iv_regress(y ~ x_exog + x_endog | x_exog + z_instr, df)`.
  - [ ] Diagnóstico de instrumentos débiles: F-statistic de primera etapa ($F > 10$).
  - [ ] Test de endogeneidad de Wu-Hausman y test de sobreidentificación de Sargan.
- [ ] **Regresión Regularizada Penalizada (Lasso, Ridge, ElasticNet):**
  - [ ] Algoritmo de descenso por coordenadas (*Coordinate Descent* estilo `glmnet`) para penalizaciones $L_1$ y $L_2$.
  - [ ] Búsqueda óptima de parámetro de regularización $\lambda$ mediante validación cruzada $K$-fold (`cv_glmnet`).

---

## Parte C: `spring_pact` — Primera Librería Oficial 100% GHL

Diseño e implementación de la primera librería de la comunidad escrita completamente en GHL para contratos estadísticos y protocolos de expectativas ([`RFC 14`](../design/14-statistical-contracts-and-expectation-protocols.md)):

- [ ] **Estructura y Manifiesto de Paquete:**
  - [ ] Creación del paquete mediante `ghl new spring_pact`.
  - [ ] Definición del manifiesto canónico `ghl.toml` y resolución reproducible con `ghl fetch` (`ghl.lock`).
- [ ] **Módulo de Expectativas de Datos (`data_rules.gh`):**
  - [ ] `expect_range(col, min, max)`: Verificación de rangos válidos admisibles (ej. edad $\ge 18$).
  - [ ] `expect_na_max(col, threshold)`: Tolerancia porcentual de valores faltantes.
  - [ ] `expect_outlier_rule(col, method, factor)`: Detección y tratamiento de casos extremos (IQR / Z-score).
  - [ ] `enforce_data_rules(df, contract)`: Pipeline de saneamiento conforme al pacto.
- [ ] **Módulo de Expectativas de Modelos (`model_rules.gh`):**
  - [ ] `expect_vif_max(threshold)`: Auditoría de multicolinealidad entre predictores.
  - [ ] `expect_homoscedasticity(p_min)`: Exigencia de matrices de covarianza robustas (HC0–HC3) si hay heterocedasticidad.
  - [ ] `expect_min_sample_size(min_n)`: Restricción de muestra mínima efectiva para inferencia.
- [ ] **Sello Criptográfico y Certificado de Cumplimiento (`certificate.gh`):**
  - [ ] Generación de hash determinista **SHA-256** del contrato pactado.
  - [ ] Emisión de recibo formal de cumplimiento (`AuditReceipt`) con resumen de reglas aprobadas/falladas.
  - [ ] Renderizado en terminal mediante panel visual **Cockpit Deck** con felicitación de Haru (`(U・ᴥ・U)`).
- [ ] **Suite de Pruebas y Validación:**
  - [ ] Pruebas unitarias de la librería ejecutadas directamente con `ghl test`.
