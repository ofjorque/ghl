### /ᐠ˵- ⩊ -˵マ ✧ Implementación y Cierre de Issue #9: Paquete `ghl_impute`

Se ha implementado, verificado y consolidado exitosamente el paquete de utilidad [`ghl_impute`](packages/ghl_impute) para tratamiento científico de datos faltantes, integrando análisis de patrones de pérdida (MCAR, MAR, MNAR), imputación múltiple por ecuaciones encadenadas (MICE / PMM), k-Nearest Neighbors con distancia normalizada de Gower, diagnósticos VIM/VIF y reglas de combinación de Rubin (1987) con corrección de grados de libertad para muestras pequeñas de Barnard-Rubin (1999).

---

#### 1. Módulos y Algoritmos Implementados
- **Análisis de Patrones y Mecanismos de Pérdida (`patterns.gh`):**
  - Detección de patrones MCAR, MAR y MNAR.
  - Métricas de pérdida por variable y resumen de frecuencias con `missing_summary` y `missing_columns`.
- **Visualización e Imputación VIM (`vim.gh`):**
  - Agregación tabular de combinaciones de patrones (`vim_aggr`) equivalente a `VIM::aggr`.
  - Matrixplot en terminal Unicode/ASCII (`vim_matrixplot`, `vim_matrixplot_sorted`) para inspección visual directa.
  - Imputación Hot-Deck estratificada (`vim_hotdeck`) con selección de donantes observados.
- **Predictive Mean Matching PMM (`pmm.gh`):**
  - Imputación semiparamétrica basada en distancias métricas sobre valores predichos, garantizando que nunca se imputen valores fuera del soporte empírico observado.
- **k-Vecinos más Cercanos k-NN (`knn.gh`):**
  - Métrica de proximidad normalizada de Gower adaptable a variables heterogéneas, con interpolación ponderada por el inverso de la distancia.
- **MICE con Arquitectura Plug-in de Modelos Universales (`mice.gh`):**
  - Imputación multivariada encadenada con $m$ cadenas de Gibbs y soporte nativo para closures de usuario como algoritmos arbitrarios de imputación (e.g. modelos personalizados, machine learning o psicométricos).
- **Reglas de Combinación de Rubin y Barnard-Rubin (`pool.gh`):**
  - Agregación de parámetros $\bar{Q}$, varianza intra-imputación $\bar{U}$, varianza entre-imputaciones $B$ y varianza total $T = \bar{U} + (1 + 1/m)B$.
  - Cálculo de incremento relativo en varianza (RIV), fracción de información faltante (FMI) y grados de libertad ajustados de Barnard & Rubin (1999) $\nu_{adj}$.
- **Diagnósticos de Colinealidad VIF (`vif.gh`):**
  - Factor de Inflación de la Varianza (VIF) sobre matrices de predictores para evitar singularidad en cadenas de imputación.

---

#### 2. Suite de Integración y Pruebas
- **Pruebas en GHL (`packages/ghl_impute/tests/impute_test.gh`):**
  - **7/7 bloques de prueba superados al 100%:**
    1. Patrones de datos faltantes y resúmenes tabulares.
    2. Diagnóstico de multicolinealidad con VIF.
    3. Imputación k-NN ponderada por distancia Gower.
    4. MICE con Predictive Mean Matching (m=3 datasets sin NA remanentes).
    5. Universal Model Plug-in (ejecución de closures arbitrarios de usuario).
    6. Reglas de Rubin para agregación de modelos con Barnard-Rubin df.
    7. VIM aggregations, matrix plot y hot-deck.

---

#### 3. Soporte en el Compilador y Runtime
- **Compilador `ghl-types`:**
  - Soporte de anotación de tipo `any` / `Any` en `Type::from_annotation`.
  - Soporte de tipo `Record` en firmas de funciones unificables con registros estructurales anónimos.
  - Normalización de campos declarados como `Type::Any` en literales de struct para permitir vectores homogéneos de especificaciones polimórficas (`MethodSpec`).
  - Registro de la función diagnóstica `assert` en `TypeEnv`.
- **Runtime `ghl-runtime`:**
  - Registro de `assert` con alias a `pounce` y fallback a mensajes diagnósticos.

---

#### 4. Entregables Documentales y Cuadernos
- **Guía Técnica Exhaustiva:** [`packages/ghl_impute/GUIDE.md`](packages/ghl_impute/GUIDE.md) documentando la taxonomía de Little & Rubin, algoritmos PMM/k-NN, reglas de Rubin y paridad/benchmarks contra R `mice`/`VIM` y Python `IterativeImputer`.
- **Cuaderno Reproducible Quarto:** [`examples/imputation_analysis.qmd`](examples/imputation_analysis.qmd).
- **Tarjetas Cockpit Deck:** Paneles de terminal de alta fidelidad `vim_cockpit`, `impute_cockpit`, `pool_cockpit` y `vif_cockpit` con badges felinos interactivos.
