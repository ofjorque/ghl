# Suite 03: Modelado Estadístico y Métodos MCMC en GHL

- **Área:** Algoritmos Estadísticos / Inferencia Bayesiana / Optimización Numérica
- **Objetivo:** Evaluar el rendimiento en problemas estadísticos reales donde los bucles de usuario, la generación de números aleatorios y la gestión de memoria por iteración son el factor crítico.

---

## 1. Casos de Prueba Especificados

### Caso 3.1: Muestreador MCMC Personalizado (Gibbs Sampler Jerárquico)
- **Operación:** 100.000 iteraciones de muestreo Gibbs con actualización secuencial de parámetros condicionales:
  - Actualización de medias de grupo $\mu_j \sim \mathcal{N}(\dots)$.
  - Actualización de precisión global $\tau \sim \text{Gamma}(\dots)$.
- **Aspecto Evaluado:** Rendimiento de bucles escalares nativos, generación de números pseudoaleatorios de alto rendimiento y presión sobre el recolector de basura (ausente en GHL gracias a las arenas de iteración).
- **Por qué importa:** En Python y R, este código puro tarda minutos debido al intérprete, obligando a usar librerías externas en C++ (como Stan). En Julia y GHL se evalúa la capacidad de escribir el algoritmo directamente.

### Caso 3.2: Ajuste de Modelos Lineales Generalizados (GLM - Regresión Logística)
- **Operación:** Ajuste por Mínimos Cuadrados Ponderados Iterativos (IRLS) sobre una matriz de diseño de $N = 1.000.000$ observaciones y $P = 40$ predictores continuos.
- **Aspecto Evaluado:** Eficiencia del solver matricial, estabilidad numérica y soporte de la sintaxis de fórmulas `y ~ x1 + ... + xP`.

### Caso 3.3: Remuestreo Bootstrap No Paramétrico
- **Operación:** 20.000 réplicas bootstrap para calcular el intervalo de confianza de un estimador robusto sobre $N = 100.000$ observaciones.
- **Aspecto Evaluado:** Escalabilidad del paralelismo multihilo de memoria compartida sin duplicación de la muestra de datos base.

### Caso 3.4: Algoritmo EM (Expectation-Maximization) para Mezcla de Gaussianas
- **Operación:** 500 iteraciones de ajuste EM para $K = 10$ componentes gaussianas multivariadas en dimensión $D = 20$.
- **Aspecto Evaluado:** Cálculo matricial repetitivo, estabilidad numérica en log-sum-exp y vectorización.

---

## 2. Implementación Real en GHL (`gibbs.gh`)

La siguiente implementación está completamente operativa en el runtime de GHL (`ghl-runtime`). Emplea primitivas generales de colecciones y álgebra lineal (`zeros`, `len`, `filter`, `get`, `set`, `set_row`, `dot`, `random_normal`, `random_gamma`):

```ghl
fn run_gibbs(y, groups, num_iterations) {
    let num_groups = max(groups) + 1;
    let mut trace = zeros(num_iterations, num_groups);
    let mut tau = 1.0;
    let mut mu_vec = zeros(num_groups);
    let n_total = len(y);

    let mut iter = 0;
    while iter < num_iterations {
        // 1. Actualización de medias grupales
        let mut j = 0;
        while j < num_groups {
            let y_j = filter(y, groups == j);
            let n_j = len(y_j);
            let post_mean = (sum(y_j) * tau) / (n_j * tau + 1.0);
            let post_sd = 1.0 / sqrt(n_j * tau + 1.0);
            let draw = first(random_normal(1, post_mean, post_sd));
            mu_vec = set(mu_vec, j, draw);
            j = j + 1;
        };

        // 2. Actualización de la precisión global tau (vector gather + dot product)
        let diff = y - get(mu_vec, groups);
        let ssq = dot(diff, diff);
        let alpha_post = 1.0 + n_total / 2.0;
        let beta_post = 1.0 + ssq / 2.0;
        tau = first(random_gamma(1, alpha_post, beta_post));

        trace = set_row(trace, iter, mu_vec);
        iter = iter + 1;
    };

    trace
}
```

---

## 3. Resultados de Benchmark (`spike_gibbs_mcmc_latency`)

- **Comando:** `cargo run --release --example spike_gibbs_mcmc_latency -p ghl-runtime -- 500 1000`
- **Configuración:** $N = 3.000$ (3 grupos de 1.000 observaciones), 500 iteraciones MCMC.
- **Parámetros reales:** $\mu = [3.0, 8.0, -4.0]$, $\sigma = 0.8$ ($\tau \approx 1.5625$).

### Medias Posteriores Recuperadas
- $\mu_0$: `3.0021` (esperado: `~3.0000`, error: `+0.0021`)
- $\mu_1$: `8.0162` (esperado: `~8.0000`, error: `+0.0162`)
- $\mu_2$: `-3.9839` (esperado: `~-4.0000`, error: `+0.0161`)

### Métricas de Rendimiento
- **Tiempo total (500 iteraciones sobre 3.000 observaciones):** `24.22 ms`
- **Latencia por iteración:** `~48.4 µs`
- **Throughput:** `~20.640 iteraciones / segundo`

---

## 4. Resultados de Benchmark: Caso 3.2 (`spike_irls_latency`)

- **Comando:** `cargo run --release --example spike_irls_latency -p ghl-runtime`
- **Configuración:** $N = 1.000.000$ observaciones, $P = 40$ predictores continuos.
- **Operación:** Ajuste GLM logístico por Mínimos Cuadrados Ponderados Iterativos (IRLS) vía NEKO.

### Precisión y Convergencia
- **Iteraciones hasta convergencia:** 5 iteraciones ($\text{tol} = 10^{-8}$).
- **Estabilidad numérica:** $\max |\beta_{\text{est}} - \beta_{\text{true}}| = 0.0044$.

### Métricas de Rendimiento
- **Tiempo total de ajuste (`fit_logistic`):** `533.39 ms` (incluyendo inferencia, cálculo de deviance y covarianzas).
- **Ensamblado de ecuaciones normales ($X^T W X$ / $X^T W z$ por iteración):**
  - Secuencial: `153.78 ms`
  - Paralelo (Rayon): `63.98 ms`
  - **Speedup multinúcleo:** `~2.40x`

---

## 5. Resultados de Benchmark: Caso 3.3 (`spike_bootstrap_mean_latency`)

- **Comando:** `cargo run --release --example spike_bootstrap_mean_latency -p ghl-runtime -- 100000 2000`
- **Configuración:** $N = 100.000$ observaciones base, $2.000$ réplicas bootstrap.
- **Operación:** Remuestreo no paramétrico multihilo sin clonación de la base (memoria compartida pura con saltos de PRNG independientes vía `Xoshiro256PlusPlus::jump`).

### Métricas de Rendimiento
- **Tiempo secuencial:** `510.20 ms`
- **Tiempo paralelo (Rayon):** `44.96 ms`
- **Speedup paralelo:** `~11.35x`
- **Extrapolación a escala completa (20.000 réplicas × 100.000 obs):** `~0.4 s` paralelo en máquina de usuario.

---

## 6. Implementación y Resultados de Benchmark: Caso 3.4 (`spike_em_gmm_latency`)

- **Operación:** Algoritmo EM (*Expectation-Maximization*) para mezclas gaussianas multivariadas con $K = 10$ componentes en dimensión $D = 20$, sobre $N = 1.000$ observaciones.
- **Aspectos evaluados:** Cálculo matricial repetitivo (multiplicación GEMM $\Gamma^T X$ para actualización de parámetros), estabilidad numérica en `log_sum_exp` y vectorización.

### Implementación en GHL (`em_gmm.gh`)
```ghl
fn run_gmm_em(X, K, D, max_iter) {
    let N = len(X);
    let mut pi = zeros(K);
    let mut k_init = 0;
    while k_init < K {
        pi = set(pi, k_init, 1.0 / (K * 1.0));
        k_init = k_init + 1;
    };

    let mut mu = zeros(K, D);
    let step = N / K;
    let mut k_mu = 0;
    while k_mu < K {
        mu = set_row(mu, k_mu, get_row(X, k_mu * step));
        k_mu = k_mu + 1;
    };

    let mut vars = zeros(K, D);
    let mut k_var = 0;
    while k_var < K {
        let mut d_init = 0;
        let mut v_init = zeros(D);
        while d_init < D {
            v_init = set(v_init, d_init, 1.0);
            d_init = d_init + 1;
        };
        vars = set_row(vars, k_var, v_init);
        k_var = k_var + 1;
    };

    let two_pi = 6.283185307179586;
    let log_two_pi_d = (D * 1.0) * log(two_pi);

    let mut iter = 0;
    while iter < max_iter {
        // --- E-step (evaluación con log_sum_exp estable) ---
        let mut Gamma = zeros(N, K);
        let mut i = 0;
        while i < N {
            let xi = get_row(X, i);
            let mut log_p = zeros(K);

            let mut c = 0;
            while c < K {
                let diff = xi - get_row(mu, c);
                let mahal = sum((diff * diff) / get_row(vars, c));
                let log_det = sum(log(get_row(vars, c)));
                let log_gauss = -0.5 * (log_two_pi_d + log_det + mahal);
                log_p = set(log_p, c, log(get(pi, c)) + log_gauss);
                c = c + 1;
            };

            let lse = log_sum_exp(log_p);
            let mut c2 = 0;
            while c2 < K {
                Gamma = set(Gamma, i, c2, exp(get(log_p, c2) - lse));
                c2 = c2 + 1;
            };
            i = i + 1;
        };

        // --- M-step (vectorización via GEMM Gamma^T * X) ---
        let Gamma_T = transpose(Gamma);
        let weighted_X = Gamma_T * X;

        let mut c3 = 0;
        while c3 < K {
            let gamma_c = get_row(Gamma_T, c3);
            let N_c = sum(gamma_c);
            pi = set(pi, c3, N_c / N);
            let new_mu_c = get_row(weighted_X, c3) / N_c;
            mu = set_row(mu, c3, new_mu_c);

            let mut sum_sq = zeros(D);
            let mut i2 = 0;
            while i2 < N {
                let diff2 = get_row(X, i2) - new_mu_c;
                sum_sq = sum_sq + (diff2 * diff2) * get(gamma_c, i2);
                i2 = i2 + 1;
            };
            vars = set_row(vars, c3, (sum_sq / N_c) + zeros(D) + 0.001);
            c3 = c3 + 1;
        };

        iter = iter + 1;
    };

    mu
}
```

### Métricas de Rendimiento (`spike_em_gmm_latency`)
- **Comando:** `cargo run --release --example spike_em_gmm_latency -p ghl-runtime -- 20 1000`
- **Configuración:** $K = 10$, $D = 20$, $N = 1.000$.
- **Tiempo total (20 iteraciones):** `4.24 s`
- **Latencia por iteración completa (E + M steps):** `~212 ms`
- **Estabilidad numérica:** `log_sum_exp` previene bajo flujo numérico o overflow durante todo el ciclo EM.
- **Convergencia:** Recuperación de medias de clusters multivariados confirmada en prueba de integración.



