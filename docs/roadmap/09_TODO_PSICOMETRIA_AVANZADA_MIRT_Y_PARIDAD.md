# Roadmap 09 — Psicometría Computacional Avanzada, Sintaxis Unificada SEM-IRT y Paridad Total con `mirt`

> Backlog temático activo derivado de [`TODO.md`](../../TODO.md), [`RFC 11`](../design/11-neko-statistical-modeling-framework.md) y [`RFC 15`](../design/15-kernel-primitives-and-self-hosted-standard-library.md).
> Aborda la evolución del paquete `ghl_irt`, la unificación de sintaxis de fórmulas SEM para restricciones de ítems, y el cierre de brechas avanzadas frente al estándar de oro de R (`mirt` de R. Philip Chalmers).

---

## 0. Reconocimiento de Profundidad: ¿Por qué `mirt` es el Estándar de Oro?

Una comparativa honesta debe reconocer que `mirt` no es solo una colección de estimadores aislados, sino un motor de optimización multiparamétrica altamente flexible desarrollado durante más de una década:
1. **Modelado Multigrupo Interno:** En `mirt`, el análisis de DIF no se limita a tablas de contingencia post-hoc $2 \times 2$ (Mantel-Haenszel), sino que calibra modelos multigrupo simultáneos (`multipleGroup`) aplicando Tests de Razón de Verosimilitud (LRT) sobre pendientes ($a$) e interceptos ($d$) para aislar DIF uniforme vs no-uniforme.
2. **Restricciones y Pries Bayesianos:** Permite fijar parámetros individuales, imponer restricciones de igualdad entre ítems (`item1.a == item2.a`), y definir distribuciones a priori (MAP) sobre adivinanza y pendientes.
3. **Equating por Curvas de Test:** Soporta métodos basados en función característica del test (TCC) como Stocking-Lord y Haebara, no solo igualación lineal de momentos (Mean-Sigma / Mean-Mean).
4. **Matrices de Diseño y CFA:** Soporta especificación libre de matrices Q factoriales exploratorias y confirmatorias.

Este roadmap define el camino técnico para que `ghl_irt` implemente estas capacidades de forma nativa en GHL, aprovechando la infraestructura de fórmulas del lenguaje.

---

## Parte A: Sintaxis de Fórmulas SEM para Modelos IRT y Restricciones

Integración del parser de fórmulas SEM (`=~` y `~~`) en la definición de especificaciones psicométricas:

- [ ] **Sintaxis de Medición y Factores (`irt_spec`):**
  - [ ] Habilitar especificación de factores latentes usando el operador `=~` de GHL:
    ```ghl
    let spec = irt_spec {
        Math =~ m1 + m2 + m3 + m4;
        Verbal =~ v1 + v2 + v3 + v4;
        Math ~~ Verbal; // Correlación entre dimensiones latentes
    };
    ```
  - [ ] Reconocimiento de restricciones de igualdad entre ítems:
    ```ghl
    m1.a == m2.a; // Discriminaciones idénticas
    ```
  - [ ] Fijación de parámetros constantes:
    ```ghl
    m3.a == 1.0;   // Restricción Rasch/1PL para m3
    m4.c == 0.20;  // Parámetro de adivinanza fijo al 20%
    ```
- [ ] **Motor de Calibración con Restricciones:**
  - [ ] Mapeo de parámetros libres vs restringidos en el vector de optimización $\boldsymbol{\theta}$.
  - [ ] Paso M restringido en el algoritmo EM preservando la estructura del modelo.

---

## Parte B: Diagnósticos Avanzados de Ajuste y Dependencia Local

- [ ] **Estadístico $M_2$ de Bondad de Ajuste Global (Maydeu-Olivares & Joe, 2005):**
  - [ ] Implementar el estadístico de información limitada de 2do orden $M_2$ para tablas de contingencia multidimensionales dispersas ($2^J$).
  - [ ] Cálculo de errores de aproximación categóricos: RMSEA derivado de $M_2$, CFI y TLI para modelos IRT.
  - [ ] Presentación en el Cockpit con umbrales de corte psicométricos ($M_2$ p-valor $> 0.05$, $\text{RMSEA} < 0.06$).
- [ ] **Matriz de Residuos $Q_3$ de Yen (1984) — Dependencia Local de Ítems (LID):**
  - [ ] Cálculo de residuos individuales de ítem: $e_{ij} = Y_{ij} - P_j(\hat{\theta}_i)$.
  - [ ] Matriz de correlación de Pearson inter-ítem $Q_3(j, k) = \text{Cor}(e_{\cdot j}, e_{\cdot k})$.
  - [ ] Detección y alerta de pares de ítems con dependencia local excesiva ($Q_3 > 0.20$ por encima de la media residual).
- [ ] **Estadístico de Ajuste de Ítem $S-X^2$ (Orlando & Thissen, 2000):**
  - [ ] Agrupación por puntuación total observada $k = 0 \dots J$ con cálculo de frecuencias esperadas bajo el modelo marginal.

---

## Parte C: Modelos Estructurales Complejos y Bi-Factor

- [ ] **Modelo Bi-Factor Canónico (`bfactor`):**
  - [ ] Descomposición ortogonal con 1 factor general ($G$) que carga en todos los ítems y $K$ factores específicos ortogonales ($s_1, \dots, s_K$) que cargan en subconjuntos disjuntos de ítems:
    $$P(Y_{ij}=1 \mid \theta_G, \theta_{sk}) = \frac{1}{1 + \exp\left(-(a_{jG}\theta_G + a_{js}\theta_{sk} + d_j)\right)}$$
  - [ ] Dimension Reduction de Gibbons & Hedeker (1992): evaluación de la verosimilitud evaluando solo integrales bidimensionales $(G, s_k)$ en lugar de $K+1$ dimensiones.
  - [ ] Cálculo de métricas psicométricas bi-factor:
    - Varianza Común Explicada: $ECV$ (*Explained Common Variance*).
    - Confiabilidad Jerárquica: $\omega_h$ (*Omega Hierarchical*) y $\omega_t$ (*Omega Total*).

---

## Parte D: DIF Multigrupo Formal por Razón de Verosimilitud (LRT)

- [ ] **Calibración Simultánea Multigrupo (`multiple_group_irt`):**
  - [ ] Estimación conjunta con distribución de habilidad libre en el grupo focal ($\mu_{\text{foc}}, \sigma_{\text{foc}}^2$) fijando el grupo de referencia en $N(0, 1)$.
  - [ ] Test LRT para DIF Uniforme: comparación de verosimilitudes restringiendo interceptos $d_{\text{ref}} = d_{\text{foc}}$ vs modelo libre ($\Delta \chi^2$ con 1 g.l.).
  - [ ] Test LRT para DIF No-Uniforme: comparación restringiendo pendientes $a_{\text{ref}} = a_{\text{foc}}$ ($\Delta \chi^2$ con 1 g.l.).
  - [ ] Test conjunto omnibús ($\Delta \chi^2$ con 2 g.l. para $a$ y $d$).

---

## Parte E: Muestreador MHRM para Alta Dimensionalidad ($>5$ Dimensiones)

- [ ] **Algoritmo Metropolis-Hastings Robbins-Monro (Cai, 2010):**
  - [ ] Sustitución de la cuadratura numérica cartesiana (que explota exponencialmente $Q^D$) por un muestreador estocástico MHRM.
  - [ ] Fase 1: Cadena M-H para muestrear del posterior $p(\boldsymbol{\theta} \mid \mathbf{Y})$.
  - [ ] Fase 2: Actualización estocástica de parámetros del modelo con paso decreciente de Robbins-Monro ($\gamma_t = 1 / t^\alpha$).
  - [ ] Habilitación de modelos confirmatorios de 5 a 20 dimensiones en tiempo lineal.

---

## Parte F: Modelos Mixtos IRT (`mixedmirt`)

- [ ] **Incorporación de Covariables y Efectos Aleatorios:**
  - [ ] Predicción de dificultad del ítem a partir de propiedades de diseño (LLTM - *Linear Logistic Test Model* de Fischer):
    $$b_j = \sum_{p=1}^P \beta_p Q_{jp} + c$$
  - [ ] Incorporación de predictores a nivel de examinado (género, nivel educativo, escuela) en la distribución latente de habilidad (*Latent Regression IRT*).
