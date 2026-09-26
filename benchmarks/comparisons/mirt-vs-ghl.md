# Comparativa Funcional y Arquitectónica: `mirt` (R) vs `ghl_irt` (GHL)

- **Área:** Psicometría Computacional / Teoría de Respuesta al Ítem (IRT) & CAT
- **Paquete Referente (R):** `mirt` v1.47 (R. Philip Chalmers) + ecosistema (`catR`, `difR`, `plink`)
- **Paquete GHL:** `ghl_irt` (Capa 1 auto-alojada en GHL + primitivas numéricas de Capa 0)
- **Roadmap Activo de Paridad:** [`docs/roadmap/09_TODO_PSICOMETRIA_AVANZADA_MIRT_Y_PARIDAD.md`](../../docs/roadmap/09_TODO_PSICOMETRIA_AVANZADA_MIRT_Y_PARIDAD.md)
- **Datasets de Prueba Exportados:** `LSAT7`, `LSAT6`, `Science`, `Bock1997`, `deAyala` (en `benchmarks/data/`)

---

## 1. Visión General y Filosofía de Diseño

| Dimensión | `mirt` (R) | `ghl_irt` (GHL) |
|---|---|---|
| **Ecosistema y Lenguaje** | Híbrido: scripts R + C++ interno vía `RcppArmadillo` + `RcppParallel`. | **100% GHL puro** con optimizaciones del compilador GHL (SIMD + CoW in-place). |
| **Dependencias Externas** | Requiere compiladores C++, BLAS/LAPACK, y ~16 paquetes CRAN (`lattice`, `stats4`, etc.). | **Cero dependencias externas**: funciona nativamente en el runtime de GHL. |
| **Estructuras de Datos** | Clases S4 pesadas (`SingleGroupClass`, `MultipleGroupClass`). | `struct` tipados estáticos de GHL (`DichotomousIrtResult`, `PolytomousIrtResult`, etc.). |
| **Diagnósticos en Consola** | Tablas de texto plano con avisos o warnings estándar de R. | **Cockpits ASCII de alta fidelidad** con sparklines Unicode (`  ▂▅▇███▇▅▂ `) y telemetría felina canónica (Gojo `≽(◉˕ ◉ ≼マ` y Haru `/ᐠ˵- ⩊ -˵マ ✧`). |
| **Ámbito Funcional** | Calibración IRT clásica y multidimensional. Requiere paquetes satélite para CAT (`mirtCAT` / `catR`), DIF clásico (`difR`) y Equating (`plink`, `equate`). | **Suite integrada de extremo a extremo**: incluye Dicotómicos, Politómicos, DIF, Person Fit, Equating, MIRT y CAT en un solo paquete. |

---

## 2. Análisis Crítico de Profundidad: ¿Dónde Radica la Ventaja Interna de `mirt`?

Una comparativa técnica rigurosa debe distinguir entre **calibrar un modelo puntual** y **la profundidad de parametrización interna** que ofrece `mirt`. A continuación se contrastan las capacidades internas reales:

### A. Extracción de Coeficientes (`coef`)
- **En `mirt`:** La función `coef()` es un subsistema completo de inferencia:
  - Soporta intervalos de confianza asintóticos (`CI = 0.95`).
  - Errores estándar derivados de matrices de información observada (Hessian, Oakes, SEM, Richardson).
  - Rotaciones ortogonales y oblicuas para matrices de carga factorial (Varimax, Promax, Oblimin, Geomin).
  - Extracción de medias y matrices de covarianza latente en modelos multidimensionales o multigrupo.
- **En `ghl_irt` (Fase 1):** `coef_irt()` entrega un `DataFrame` con las estimaciones puntuales de parámetros ($a, b, c, d$). La inferencia asintótica completa (matriz de covarianza $V(\hat{\boldsymbol{\xi}})$ e intervalos de confianza) se aborda en el Roadmap 09.

### B. Diagnóstico de Ajuste de Ítems (`itemfit`)
- **En `mirt`:** Soporta el estadístico de referencia moderna $S-X^2$ de Orlando & Thissen (2000), agrupando examinados por su puntaje total observado exacto para evitar el sesgo de cuadratura, además de $PV-Q_1$, infit/outfit y curvas empíricas de proporción observada.
- **En `ghl_irt` (Fase 1):** Calcula los estadísticos de residuos cuadráticos estandarizados medios (Infit y Outfit MSQ) y Pearson $\chi^2$ global.

### C. Ajuste de Sujetos (*Person Fit*)
- **En `mirt`:** `personfit()` computa no solo el índice $Z_h$ de Drasgow, sino también infit y outfit a nivel de sujeto individual, log-verosimilitud observada y estadísticos bajo patrones agrupados.
- **En `ghl_irt` (Fase 1):** Implementa el estadístico $Z_h$ estandarizado de Drasgow con identificación de anomalías para $Z_h < -2.0$ y reporte en Cockpit.

### D. Funcionamiento Diferencial del Ítem (*DIF*)
- **En `mirt`:** El enfoque principal de `mirt::DIF()` es el **Test de Razón de Verosimilitud (LRT)** dentro del modelo de calibración multigrupo (`multipleGroup`). Permite restringir parámetros específicos (`which.par = c('a1', 'd')`) para contrastar formalmente hipótesis de:
  - **DIF Uniforme:** diferencias en dificultad/intercepto ($d$).
  - **DIF No-Uniforme:** diferencias en pendiente/discriminación ($a$).
- **En `ghl_irt` (Fase 1):** Implementa el método no-paramétrico clásico de **Mantel-Haenszel** estratificado con la métrica ETS Delta ($\Delta_{MH}$) y clasificación A/B/C, más el test de Wald de Lord sobre parámetros estimados. El modelado multigrupo simultáneo con LRT forma parte de Roadmap 09.

### E. Equiparación y Enlace de Escalas (*Equating & Linking*)
- **En R (`mirt` / `plink` / `equate`):** Además de métodos lineales de momentos (Mean-Sigma, Mean-Mean), soporta métodos no-lineales basados en la curva característica del test (TCC) de **Stocking-Lord** y **Haebara**, que minimizan la distancia cuadrática integrada entre curvas esperadas.
- **En `ghl_irt` (Fase 1):** Implementa transformaciones lineales de escala por igualación de momentos (Mean-Sigma y Mean-Mean).

### F. Modelos Multidimensionales (*MIRT*)
- **En `mirt`:** Soporta especificación libre de matrices Q factoriales confirmatorias, modelos exploratorios con rotaciones arbitrarias, correlación libre entre rasgos latentes, modelos parcialmente compensatorios y descomposiciones bi-factor.
- **En `ghl_irt` (Fase 1):** Calibra modelos M2PL compensatorios con dimensiones ortogonales, cálculo de métricas de Reckase ($MDISC$ y $MDIFF$) y puntuación EAP bidimensional.

---

## 3. Matriz de Mapeo de Funciones

| Familia Psicométrica | Función en `mirt` (R) / Ecosistema R | Función en `ghl_irt` (GHL) | Nivel de Cobertura |
|---|---|---|---|
| **1PL / Rasch** | `mirt(data, 1, itemtype = 'Rasch')` | `fit_1pl(df, items)` | Completa (EM + cuadratura). |
| **2PL (Birnbaum)** | `mirt(data, 1, itemtype = '2PL')` | `fit_2pl(df, items)` | Completa (EM + cuadratura). |
| **3PL (Adivinanza)** | `mirt(data, 1, itemtype = '3PL')` | `fit_3pl(df, items)` | Completa ($a, b, c$). |
| **4PL (Desatención)** | `mirt(data, 1, itemtype = '4PL')` | `fit_4pl(df, items)` | Completa ($a, b, c, d$). |
| **GRM (Samejima)** | `mirt(data, 1, itemtype = 'graded')` | `fit_grm(df, items, n_cats)` | Completa (probabilidades cumulativas). |
| **GPCM (Muraki)** | `mirt(data, 1, itemtype = 'gpcm')` | `fit_gpcm(df, items, n_cats)` | Completa (umbrales por paso). |
| **PCM (Masters)** | `mirt(data, 1, itemtype = 'gpcm', const_slope=TRUE)` | `fit_pcm(df, items, n_cats)` | Completa ($a = 1.0$). |
| **RSM (Andrich)** | `mirt(data, 1, itemtype = 'rsm')` | `fit_rsm(df, items, n_cats)` | Completa ($\tau_k + b_j$). |
| **NRM (Bock)** | `mirt(data, 1, itemtype = 'nominal')` | `fit_nrm(df, items, n_cats)` | Completa ($a_{jk}, c_{jk}$). |
| **Extracción de Parámetros** | `coef(mod, IRTpars = TRUE)` | `coef(model)`, `coef_irt(model)` | Puntual en GHL; inferencial/rotaciones en `mirt`. |
| **Ajuste de Ítem** | `itemfit(mod)` | `compute_item_fit(...)` | Infit/Outfit en ambos; $S-X^2$ en `mirt`. |
| **Ajuste de Sujetos** | `personfit(mod)` | `compute_person_fit_zh(...)` | $Z_h$ de Drasgow idéntico en ambos. |
| **DIF** | `DIF(mod)` / `difR::difMH` | `compute_mantel_haenszel_dif(...)` | Mantel-Haenszel + ETS Delta en GHL; LRT multigrupo en `mirt`. |
| **Equating** | `plink` / `equate` | `link_scales_mean_sigma`, `link_scales_mean_mean` | Lineal (Mean-Sigma/Mean) en GHL; Curvas TCC en R. |
| **MIRT Multidimensional** | `mirt(data, 2)` | `fit_m2pl(df, items, n_dims)` | M2PL compensatorio + Reckase en GHL; CFA/Q-matrix en `mirt`. |
| **Test Adaptativo (CAT)** | `mirtCAT` / `catR` | `simulate_cat`, `select_next_item_mfi` | MFI + parada SEM + simulación con sparklines. |

---

## 4. Próxima Frontera: Sintaxis Unificada SEM para IRT

Para superar las limitaciones de sintaxis procedural y permitir restricciones ricas como en `mirt`, GHL adoptará la sintaxis de ecuaciones estructurales (desarrollada en RFC 11/15) para especificar modelos psicométricos:

```ghl
// Especificación de modelo IRT con sintaxis de fórmulas SEM en GHL:
let spec = irt_spec {
    // Factores latentes y constructos:
    Math =~ m1 + m2 + m3 + m4;
    Verbal =~ v1 + v2 + v3 + v4;
    
    // Covarianza entre dimensiones latentes:
    Math ~~ Verbal;
    
    // Restricciones de igualdad entre ítems (mismo poder de discriminación):
    m1.a == m2.a;
    
    // Fijación de parámetros (Restricción Rasch / Adivinanza fija):
    m3.a == 1.0;
    m4.c == 0.20;
};

let model = fit_mirt(spec, df);
```

---

## 5. Módulos Críticos Incorporados al Roadmap 09

Derivados de esta comparativa y formalizados en [`docs/roadmap/09_TODO_PSICOMETRIA_AVANZADA_MIRT_Y_PARIDAD.md`](../../docs/roadmap/09_TODO_PSICOMETRIA_AVANZADA_MIRT_Y_PARIDAD.md):

1. **$M_2$ (Limited Information Overall Fit):** Estadístico $\chi^2$ de Maydeu-Olivares & Joe con RMSEA y CFI categóricos para evaluar el ajuste global del test sin el sesgo de tablas $2^J$ hiper-dispersas.
2. **$Q_3$ de Yen:** Matriz de correlación de residuos para verificar empíricamente la independencia local de los ítems (*Local Item Dependence* / LID).
3. **Modelos Bi-Factor (`bfactor`):** Descomposición de un factor general ortogonal a factores específicos de dominio con métricas de varianza común explicada ($ECV$) y confiabilidad jerárquica ($\omega_h$).
4. **Algoritmo MHRM (Metropolis-Hastings Robbins-Monro):** Estimador estocástico para calibrar modelos confirmatorios de alta dimensionalidad continua ($>5$ dimensiones) sin sufrir la explosión combinatoria de la cuadratura cartesiana.
5. **Modelos Mixtos IRT (`mixedmirt`):** Integración de efectos aleatorios y covariables a nivel de persona e ítem (modelos LLTM de Fischer y regresión latente).
