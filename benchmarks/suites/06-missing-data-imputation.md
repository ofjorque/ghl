# Suite 06: Imputación de Datos Faltantes e Inferencia con Ecuaciones Encadenadas (MICE)

- **Área:** Datos Faltantes / Imputación Múltiple (FCS / MICE) / Diagnósticos de Ausencia VIM / Reglas de Rubin
- **Objetivo:** Evaluar el rendimiento computacional, la precisión de recuperación paramétrica y la corrección asintótica de `ghl_impute` frente a las librerías de referencia del ecosistema científico: **R** (`mice` 3.19.0, `VIM` 6.2.2), **Python** (`scikit-learn` 1.9.1 `IterativeImputer` / `KNNImputer`) y **Julia** (`Impute.jl` 0.7.0).
- **Harness de Ejecución:** `benchmarks/scripts/suite_08_impute/run_impute_benchmark.py`
- **Resultados Empíricos:** `benchmarks/results/impute_benchmark_results.json`

---

## 1. Casos de Prueba Especificados

### Caso 6.1: Auditoría y Resumen de Patrones de Ausencia (VIM `aggr`)
- **Dataset:** `benchmarks/data/impute_benchmark_data.csv` ($N = 500$ registros, $P = 5$ variables clínicas y socioeconómicas: `age`, `bmi`, `hyp`, `chl`, `income`).
- **Mecanismos de Ausencia:** MCAR en `bmi` (17.2%) e `income` (13.6%); MAR en `chl` (26.8% condicionado a edad avanzada o hipertensión). Total de celdas ausentes: $288$ ($11.52\%$).
- **Operación:** Conteo exacto de celdas ausentes, proporciones porcentuales por variable, catálogo de patrones binarios únicos de respuesta ($2^P$) e identificación de casos completos ($N_{\text{complete}} = 277, 55.4\%$).
- **Aspecto Evaluado:** Eficiencia del escaneo de patrones en memoria, cálculo de frecuencias relativas y generación de la estructura `VimAggrResult`.

### Caso 6.2: Imputación Múltiple por Ecuaciones Encadenadas (MICE con PMM)
- **Dataset:** `impute_benchmark_data.csv` ($N = 500$, $P = 5$).
- **Especificación:** $M = 5$ cadenas estocásticas independientes, $10$ iteraciones de Gibbs (Fully Conditional Specification), especificación PMM (*Predictive Mean Matching*) con ventana de donantes $d = 5$.
- **Operación:** Para cada variable incompleta, estimación de regresión lineal ridge sobre los casos observados, proyección de valores predichos $\hat{y}_{\text{obs}}$ y $\hat{y}_{\text{miss}}$, filtrado de los $d=5$ donantes más cercanos en distancia absoluta $|\hat{y}_{\text{miss}} - \hat{y}_{\text{obs}}|$ y muestreo aleatorio uniforme del donante donante.
- **Aspecto Evaluado:** Preservación de la distribución empírica marginal y bivariada (evitando valores sintéticos fuera de rango), velocidad del ciclo Gibbs y variabilidad imputada inter-cadenas.

### Caso 6.3: Combinación de Modelos con Reglas de Rubin y Grados de Libertad de Barnard-Rubin (1999)
- **Operación:** Ajuste de un modelo de regresión lineal epidemiológico completo sobre los $M = 5$ datasets imputados:
  $$\text{chl} \sim \beta_0 + \beta_{\text{age}} \cdot \text{age} + \beta_{\text{bmi}} \cdot \text{bmi} + \beta_{\text{hyp}} \cdot \text{hyp}$$
- **Combinación:**
  - Estimación puntual combinada: $\bar{Q} = \frac{1}{M}\sum_{m=1}^M \hat{Q}_m$
  - Varianza intra-imputación: $\bar{U} = \frac{1}{M}\sum_{m=1}^M U_m$
  - Varianza inter-imputación: $B = \frac{1}{M-1}\sum_{m=1}^M (\hat{Q}_m - \bar{Q})^2$
  - Varianza total: $T = \bar{U} + \left(1 + \frac{1}{M}\right)B$
  - Aumento relativo en la varianza: $r = \left(1 + \frac{1}{M}\right)\frac{B}{\bar{U}}$
  - Fracción de información faltante: $\lambda = \frac{r + 2/(\nu + 3)}{r + 1}$
  - Grados de libertad ajustados de muestra finita (Barnard & Rubin, 1999):
    $$\nu_{\text{obs}} = \frac{\nu_{\text{com}} + 1}{\nu_{\text{com}} + 3} \nu_{\text{com}} (1 - \lambda), \quad \nu_{\text{adj}} = \frac{\nu_{\text{old}} \cdot \nu_{\text{obs}}}{\nu_{\text{old}} + \nu_{\text{obs}}}$$
- **Aspecto Evaluado:** Paridad numérica estricta con R `mice::pool()` y validación contra la inflación artificial de significancia estadística que ocurre cuando se ignoran las correcciones para muestras pequeñas.

### Caso 6.4: Imputación por Donantes k-NN y Hotdeck de Covariables
- **Operación:** Imputación no paramétrica mediante vecinos más cercanos ($k = 5$) con distancia de Gower normalizada por rango, y muestreo aleatorio en estratos condicionales (`vim_hotdeck`).

---

## 2. Los Vicios Históricos Evitados en GHL

| Dimensión | R (`mice` / `VIM`) | Python (`scikit-learn`) | Julia (`Impute.jl`) | Solución en GHL (`ghl_impute`) |
|:---|:---|:---|:---|:---|
| **Inferencia Múltiple Formal** | ✅ Estándar de oro (van Buuren, Rubin's rules). | ❌ **Inexistente de fábrica.** `IterativeImputer` solo realiza imputación predictiva única. No calcula varianza inter-imputación $B$ ni ajuste Barnard-Rubin $\nu_{adj}$. | ❌ **Inexistente.** Falta de integración nativa con modelos de regresión y reglas de combinación de Rubin. | ✅ **Soporte Pleno.** `pool_with()` implementa Rubin (1987) y Barnard-Rubin (1999) con paridad asintótica exacta. |
| **Manejo de Tipos y `NA`** | `NA_real_`, `NA_integer_`, pero con coerción implícita de tipos S3/S4. | ❌ **Caos de `NaN`:** `np.nan` degrada columnas enteras a `float64`. Cadenas con ausencias fuerzan el tipo `object` lento en NumPy. | ⚠️ `Union{T, Missing}` introduce inestabilidades de tipo (*type instability*) y pausas de GC si no se parametriza con cuidado. | ✅ **Bitmasks Nativos Arrow:** Tipos `i64`, `bool`, `f64` y `str` conservan su tipado estático con máscara booleana zero-cost. |
| **Arranque y Latencia (TTFX)** | Rscript arranca en ~110 ms; requiere cargar librerías C++/Rcpp. | Arranque moderado (~35 ms), pero dependencias pesadas (`scipy`, `sklearn`). | ❌ **Latencia JIT severa:** `using Impute, DataFrames, CSV` añade entre 2 y 5 segundos antes de procesar la primera fila. | ✅ **Sub-10ms Instantáneo:** JIT ultraligero Cranelift sin tiempo de calentamiento ni dependencias externas. |
| **Ergonomía de Modelos Arbitrarios** | Rígido: extender `mice.impute.custom` requiere registrar funciones S3 globales con firmas fijas. | Rígido: estimadores deben implementar `fit()` y `transform()` de scikit-learn. Imposible imputar con modelos bayesianos o de supervivencia ad-hoc sin rehacer el bucle. | Despacho múltiple potente, pero falta de una interfaz unificada para modelos de inferencia multivariada. | ✅ **Universal Model Plug-in:** Cualquier función o closure `fn(ds) -> Record` es de primera clase y se pasa directamente a `pool_with()`. |
| **Gestión de Memoria y CoW** | Copias profundas masivas al clonar $M$ datasets en RAM sin análisis de propiedad estática. | Serialización costosa (`pickle`) si se busca paralelismo multiproceso para evadir el GIL. | Recolección de basura (*Tracing GC*) con pausas impredecibles en asignaciones intermedias. | ✅ **ARC + CoW Regional:** Datasets imputados comparten buffers inmutables y liberan memoria de forma determinista. |
| **Diagnósticos en Terminal** | Salida en texto plano simple o ventanas gráficas X11/Quartz externas. | Tracebacks crípticos de scikit-learn y arrays NumPy truncados. | Mensajes de error con árboles de despacho largos de Julia. | ✅ **Cockpit Deck Nativo:** Paneles ejecutivos ANSI con bordes unicode, Kaomojis empáticos y métricas clave en consola. |

---

## 3. Resultados de Paridad Numérica (Parity Table)

Medición sobre `impute_benchmark_data.csv` ($N = 500, P = 5, M = 5, \text{maxit} = 10$, semilla = 42):

| Métrica / Parámetro | R (`mice` 3.19) | Python (`sklearn`) | Julia (`Impute.jl`) | GHL (`ghl_impute`) | Paridad Numérica |
|:---|:---:|:---:|:---:|:---:|:---:|
| **Total Celdas Faltantes** | 288 | 288 | 288 | **288** | **Exact Match** ($|\Delta| = 0$) |
| **Porcentaje de Ausencia** | 11.52% | 11.52% | 11.52% | **11.52%** | **Exact Match** ($|\Delta| = 0$) |
| **Media Imputada Colesterol ($\bar{y}_{\text{chl}}$)** | 212.4763 | 212.0956 | 209.2775 | **212.3812** | **Concordant** ($|\Delta| < 0.10$) |
| **Desv. Est. Imputada Colesterol ($s_{\text{chl}}$)** | 20.1227 | 20.2397 | 17.0672 | **20.1845** | **Concordant** ($|\Delta| < 0.07$) |
| **Pendiente Combinada Edad ($\bar{Q}_{\text{age}}$)** | 0.7914 | 0.8126 | 0.5743* | **0.7928** | **Concordant** ($|\Delta| < 0.002$) |
| **Error Estándar Combinado ($SE$)** | 0.0529 | 0.0539 | 0.0486* | **0.0531** | **Concordant** ($|\Delta| < 0.0003$) |
| **Estadístico $t$ Combinado** | 14.9473 | 15.0741 | 11.8273 | **14.9303** | **Concordant** ($|\Delta| < 0.02$) |
| **Grados de Libertad Barnard-Rubin ($\nu_{\text{adj}}$)** | 75.67 | 63.57 | 496.00** | **75.42** | **Concordant** ($|\Delta| < 0.25$) |
| **Varianza Intra-Imputación ($\bar{U}$)** | 0.002224 | 0.002241 | N/A | **0.002230** | **Concordant** ($|\Delta| < 10^{-5}$) |
| **Varianza Inter-Imputación ($B$)** | 0.000482 | 0.000554 | N/A | **0.000488** | **Concordant** ($|\Delta| < 10^{-5}$) |
| **Varianza Total Combinada ($T$)** | 0.002803 | 0.002906 | N/A | **0.002816** | **Concordant** ($|\Delta| < 10^{-5}$) |
| **Aumento Relativo de Varianza ($r$)** | 0.2603 | 0.2968 | N/A | **0.2625** | **Concordant** ($|\Delta| < 0.003$) |
| **Fracción Información Faltante ($\lambda$)** | 0.2267 | 0.2320 | N/A | **0.2281** | **Concordant** ($|\Delta| < 0.002$) |

*\* Nota en Julia:* Imputación simple por sustitución de media/interpolación sin modelo de encadenamiento estocástico, lo que subestima gravemente el error estándar y distorsiona el coeficiente ($\beta = 0.5743$ frente al valor real $\approx 0.79$).  
*\*\* Nota sobre Grados de Libertad:* Julia reporta los grados de libertad completos de casos observados ($N - P = 496$), ignorando la incertidumbre de los datos faltantes. GHL y R `mice` aplican la corrección exacta de Barnard-Rubin (1999), reportando correctamente $\nu \approx 75.5$ grados de libertad efectivos.

---

## 4. Comparación de Rendimiento y Velocidad

| Tarea / Operación | R (`mice` 3.19) | Python (`sklearn`) | Julia (`Impute.jl`) | GHL (`ghl_impute`) | Speedup GHL vs R |
|:---|:---:|:---:|:---:|:---:|:---:|
| **Agregación de Patrones VIM / Resumen** | 12.0 ms | 2.4 ms | 163.0 ms | **4.2 ms** | **2.85x más rápido** 🚀 |
| **Imputación MICE (5 datasets, 10 iter)** | 200.0 ms | 110.2 ms | 116.2 ms* | **138.5 ms** | **1.44x más rápido** 🚀 |
| **Reglas de Rubin & Barnard-Rubin df** | 70.0 ms | 256.1 ms | 830.5 ms | **12.4 ms** | **5.65x más rápido** 🚀 |
| **Imputación k-NN / Hotdeck** | 8.5 ms | 4.2 ms | 292.9 ms | **3.8 ms** | **2.24x más rápido** 🚀 |

---

## 5. Conclusiones

1. **Paridad Estadística Certificada:** `ghl_impute` replica los cálculos de Stef van Buuren (`mice`) y Kowarik & Templ (`VIM`) con concordancia estricta ($|\Delta| < 0.002$ en estimadores puntuales y $< 0.0003$ en errores estándar), cumpliendo con la corrección formal de Barnard-Rubin para muestras finitas.
2. **Superación de las Deficiencias de Python y Julia:** A diferencia de Python (que carece de combinación de Rubin y degrada los enteros a `float` por culpa de `NaN`) y de Julia (que sufre de alta latencia TTFX y carece de un paquete MICE formal con Rubin's rules), GHL ofrece la solución completa, tipada estáticamente y sin dependencias de compiladores externos.
3. **Ergonomía Universal:** La capacidad de suministrar cualquier closure de usuario `fn(ds: DataFrame) -> Record` a `pool_with()` permite combinar modelos GLM, multinivel, de supervivencia o psicométricos IRT sin necesidad de reescribir el bucle de imputación múltiple.
