# Comparativa Funcional y Arquitectónica: `mirt` (R) vs `ghl_irt` (GHL)

- **Área:** Psicometría Computacional / Teoría de Respuesta al Ítem (IRT) & CAT
- **Paquete Referente (R):** `mirt` v1.47 (R. Philip Chalmers) + ecosistema (`catR`, `difR`, `plink`)
- **Paquete GHL:** `ghl_irt` (Capa 1 auto-alojada en GHL + primitivas numéricas de Capa 0)
- **Datasets de Prueba Exportados:** `LSAT7`, `LSAT6`, `Science`, `Bock1997`, `deAyala` (almacenados en `benchmarks/data/`)

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

## 2. Matriz Exhaustiva de Funcionalidades

### A. Modelos de Calibración Dicotómica

| Modelo | Función en `mirt` (R) | Función en `ghl_irt` (GHL) | Estado y Concordancia Matemática |
|---|---|---|---|
| **1PL / Rasch** | `mirt(data, 1, itemtype = 'Rasch')` | `fit_1pl(df, items)` | **Equivalente:** Pendiente fija $a_j = 1.0$, dificultad libre $b_j$. |
| **2PL (Birnbaum)** | `mirt(data, 1, itemtype = '2PL')` | `fit_2pl(df, items)` | **Equivalente:** Estimación de pendiente $a_j$ y dificultad $b_j$ vía EM. |
| **3PL (Adivinanza)** | `mirt(data, 1, itemtype = '3PL')` | `fit_3pl(df, items)` | **Equivalente:** Estimación de $a_j, b_j$ y cota inferior pseudo-azar $c_j$. |
| **4PL (Desatención)** | `mirt(data, 1, itemtype = '4PL')` | `fit_4pl(df, items)` | **Equivalente:** Parámetro $d_j$ de cota superior (lapsus/inattention). |

### B. Modelos Politómicos

| Modelo | Función en `mirt` (R) | Función en `ghl_irt` (GHL) | Estado y Concordancia Matemática |
|---|---|---|---|
| **GRM (Samejima)** | `mirt(data, 1, itemtype = 'graded')` | `fit_grm(df, items, n_cats)` | **Equivalente:** Curvas de respuesta cumulativa con umbrales ordenados $b_{jk}$. |
| **GPCM (Muraki)** | `mirt(data, 1, itemtype = 'gpcm')` | `fit_gpcm(df, items, n_cats)` | **Equivalente:** Discriminación variable $a_j$ y umbrales de crédito parcial $b_{jk}$. |
| **PCM (Masters)** | `mirt(data, 1, itemtype = 'gpcm', const_slope=TRUE)` | `fit_pcm(df, items, n_cats)` | **Equivalente:** Modelo de crédito parcial con pendientes idénticas $a_j = 1.0$. |
| **RSM (Andrich)** | `mirt(data, 1, itemtype = 'rsm')` | `fit_rsm(df, items, n_cats)` | **Equivalente:** Umbrales compartidos de escala Likert $\tau_k$ más dificultad $b_j$. |
| **NRM (Bock)** | `mirt(data, 1, itemtype = 'nominal')` | `fit_nrm(df, items, n_cats)` | **Equivalente:** Categorías no ordenadas con scoring libre de pendientes $a_{jk}$ e interceptos $c_{jk}$. |

### C. Extracción de Parámetros y Coeficientes

| Característica | `mirt` (R) | `ghl_irt` (GHL) | Notas de Parametrización |
|---|---|---|---|
| **Extracción Estándar** | `coef(mod)` | `coef(model)` | En `mirt` por defecto entrega intercepto $d = -a \cdot b$. En GHL entrega los coeficientes nativos. |
| **Parametrización Clásica IRT** | `coef(mod, IRTpars = TRUE)` | `coef_irt(model)` | GHL retorna directamente un `DataFrame` con columnas explícitas `item`, `a`, `b`, `c`, `d`. |
| **Matriz de Covarianza** | `vcov(mod)` | Primitiva `vcov` en Capa 0 / Hessian | `mirt` calcula SE vía Hessian numérico o Oakes/SEM. GHL provee errores estándar asintóticos. |

### D. Diagnóstico de Ajuste de Ítems (*Item Fit*)

| Diagnóstico | `mirt` (R) | `ghl_irt` (GHL) | Notas |
|---|---|---|---|
| **Infit & Outfit MSQ** | `itemfit(mod, fit_stats = 'infit')` | `compute_item_fit(df, items, model, thetas)` | Ambos calculan media de residuos cuadráticos estandarizados (Wright & Masters). |
| **Estadístico $S-X^2$** | `itemfit(mod, fit_stats = 'S_X2')` | *(Próxima extensión)* | $S-X^2$ de Orlando & Thissen agrupa por puntaje observado. GHL actualmente usa Pearson $\chi^2$. |
| **Dependencia Local ($Q_3$)** | `residuals(mod, type = 'Q3')` | *(Próxima extensión)* | Correlación de residuos de Yen ($Q_3$). |

### E. Diagnóstico de Ajuste de Personas (*Person Fit*)

| Diagnóstico | `mirt` (R) | `ghl_irt` (GHL) | Notas |
|---|---|---|---|
| **Estadístico $Z_h$ de Drasgow** | `personfit(mod)$Zh` | `compute_person_fit_zh(...)` | **Totalmente idéntico:** Log-verosimilitud estandarizada frente a la esperanza y varianza condicional. Alerta de atípicos para $Z_h < -2.0$. |
| **Cockpit de Person Fit** | No disponible (sólo matriz numérica). | `render_person_fit_cockpit(...)` | Resumen visual con conteo de sujetos atípicos e insignias de validez. |

### F. Medición de Invarianza y Sesgo (*DIF*)

| Método | En R (`mirt` o satélites) | `ghl_irt` (GHL) | Notas |
|---|---|---|---|
| **Mantel-Haenszel DIF** | `difR::difMH` (en `mirt` se usa `DIF(mod, ...)` por LRT) | `compute_mantel_haenszel_dif(...)` | Tablas de contingencia $2 \times 2 \times K$ estratificadas por score total. |
| **Escala ETS Delta ($\Delta_{MH}$)** | `difR::difMH` | `compute_mantel_haenszel_dif(...)` | **Idéntico:** $\Delta_{MH} = -2.35 \ln(\alpha_{MH})$. Clasificación formal en **Clase A**, **Clase B** y **Clase C**. |
| **Test de Wald de Lord** | `difR::difLord` | `compute_mantel_haenszel_dif(...)` | Comparación multivariada de parámetros entre grupos focal y de referencia. |
| **Cockpit de DIF** | Gráficos base en `difR`. | `render_dif_cockpit(...)` | Panel interactivo con conteo por clases y alerta de severidad de Gojo. |

### G. Equiparación de Escalas (*Equating & Linking*)

| Método | En R (`plink` / `equate`) | `ghl_irt` (GHL) | Notas |
|---|---|---|---|
| **Mean-Sigma** | `plink::plink(..., asLogical=TRUE)` | `link_scales_mean_sigma(ref_b, focal_b)` | Constantes de traslación y escala $A = \sigma_{\text{ref}}/\sigma_{\text{foc}}$, $B = \mu_{\text{ref}} - A\mu_{\text{foc}}$. |
| **Mean-Mean** | `plink::plink(...)` | `link_scales_mean_mean(...)` | Estimación de $A$ a partir de la razón de pendientes medias. |
| **Transformación Lineal** | `plink::link(...)` | `equate_parameters_linear(...)` | Transforma tanto parámetros de ítems ($a^*, b^*$) como habilidades ($\theta^*$). |

### H. Modelos Multidimensionales (*MIRT*)

| Característica | `mirt` (R) | `ghl_irt` (GHL) | Notas |
|---|---|---|---|
| **M2PL Compensatorio** | `mirt(data, 2, itemtype='2PL')` | `fit_m2pl(df, items, n_dims)` | Calibra pendientes vectoriales $(a_{j1}, a_{j2})$ e intercepto $d_j$. |
| **Métricas de Reckase** | `MDISC(mod)`, `MDIFF(mod)` | `mdisc`, `mdiff` en `MirtResult` | Discriminación multidimensional $MDISC_j = \sqrt{\sum a_{jd}^2}$ y dificultad $MDIFF_j = -d_j / MDISC_j$. |
| **Puntuación Factorial 2D** | `fscores(mod, method='EAP')` | `score_persons_mirt(...)` | Cuadratura de Gauss-Hermite en grilla cartesiana bidimensional. |

### I. Testeo Adaptativo Computarizado (*CAT*)

| Componente | En R (`mirtCAT` / `catR`) | `ghl_irt` (GHL) | Notas |
|---|---|---|---|
| **Selección de Ítems** | `mirtCAT(..., criteria = 'MI')` | `select_next_item_mfi(...)` | Regla de Máxima Información de Fisher ($MFI$) en el $\hat{\theta}$ actual. |
| **Actualización de Habilidad** | `mirtCAT(..., method = 'EAP')` | `update_cat_session(...)` | Estimación dinámica y actualización del error estándar condicional $SEM(\hat{\theta})$. |
| **Simulador de Sesión CAT** | `catR::randomCAT(...)` | `simulate_cat(...)` | Simula la trayectoria completa de un examinado dado un $\theta$ latente. |
| **Visualización de Trayectoria** | Plots de `mirtCAT` | `render_cat_cockpit(...)` | Telemetría en tiempo real con sparkline de convergencia: `▄ ▄█▇█`. |

---

## 3. Parametrizaciones Matemáticas: Puntos Clave de Conversión

Uno de los aspectos más importantes al contrastar `mirt` con cualquier librería de IRT es su convención de parametrización interna:

1. **Forma Interna de `mirt` (Slope-Intercept):**
   $$P(Y_{ij} = 1 \mid \theta) = c_j + (1 - c_j) \frac{1}{1 + \exp\left(-(a_j \theta + d_j)\right)}$$
   Aquí $d_j$ es un intercepto (a mayor $d_j$, mayor probabilidad de acierto $\implies$ ítem más fácil).

2. **Forma Clásica de IRT (`ghl_irt` y `mirt(..., IRTpars=TRUE)`):**
   $$P(Y_{ij} = 1 \mid \theta) = c_j + (d_{\text{upper}} - c_j) \frac{1}{1 + \exp\left(-D \cdot a_j (\theta - b_j)\right)}$$
   Aquí $b_j$ es la dificultad (a mayor $b_j$, menor probabilidad $\implies$ ítem más difícil).

3. **Ecuación de Enlace:**
   $$d_j = -a_j \cdot b_j \iff b_j = -\frac{d_j}{a_j}$$
   Al comparar resultados numéricos entre `mirt` y `ghl_irt`, debemos asegurarnos de comparar contra `coef(mod, IRTpars = TRUE)` para evaluar los mismos parámetros $a$ y $b$.

---

## 4. Conjuntos de Datos Estándar Disponibles para Benchmarks

Hemos extraído directamente del paquete `mirt` de R los siguientes conjuntos de datos oficiales a `benchmarks/data/`:

| Dataset | Registros | Ítems | Tipo de Respuesta | Descripción |
|---|---|---|---|---|
| **`lsat7_raw.csv`** | 1,000 | 5 | Dicotómica (0/1) | Law School Admission Test (Sección 7) expandido a nivel examinado. |
| **`lsat6_raw.csv`** | 1,000 | 5 | Dicotómica (0/1) | Law School Admission Test (Sección 6) expandido a nivel examinado. |
| **`science.csv`** | 392 | 4 | Politómica (1..4) | Escala de actitud hacia la ciencia (Likert 4 categorías). |
| **`bock1997.csv`** | 64 | 3 | Nominal | Datos nominales de Bock (1997). |
| **`deayala.csv`** | 32 | 5 | Crédito Parcial | Datos politómicos de de Ayala. |

---

## 5. Funciones de `mirt` No Implementadas Aún en GHL (Próximos Pasos Opcionales)

Para completar una paridad del 100% con las 83 funciones de `mirt`, los siguientes módulos adicionales podrían considerarse en fases posteriores:
1. **Estadístico $M_2$ de Bondad de Ajuste Global:** Estadístico de información limitada para datos categóricos de Maydeu-Olivares & Joe (`mirt::M2`).
2. **Diagnóstico de Dependencia Local $Q_3$:** Matriz de correlación de residuos de Yen (`mirt::residuals(type='Q3')`).
3. **Modelos Bi-Factor:** Análisis bi-factor con factor general y factores específicos ortogonales (`mirt::bfactor`).
4. **Algoritmo MHRM:** Muestreador Metropolis-Hastings Robbins-Monro para estimar modelos con $>5$ dimensiones continuas (`mirt(..., method='MHRM')`).
5. **Efectos Mixtos IRT:** Modelos con covariables de personas e ítems (`mirt::mixedmirt`).
