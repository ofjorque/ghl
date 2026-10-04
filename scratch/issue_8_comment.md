### /ᐠ˵- ⩊ -˵マ ✧ Implementación y Cierre de Issue #8: Paquete `ghl_optim`

Se ha implementado, verificado y consolidado exitosamente el paquete de utilidad [`ghl_optim`](packages/ghl_optim) con optimización matemática multidimensional, soporte para cotas de caja, mínimos cuadrados no lineales (NLS) y estimación de máxima verosimilitud (MLE).

---

#### 1. Algoritmos Implementados en el Runtime y Motor Numérico
- **BFGS Quasi-Newton (`minimize_bfgs` / `optim(..., "BFGS")`):** Con búsqueda lineal con retroceso bajo condiciones de Armijo ($c_1 = 10^{-4}$) y actualización de la inversa Hessiana $H_{k+1}$.
- **Nelder-Mead Simplex (`minimize_nelder_mead` / `optim(..., "Nelder-Mead")`):** Algoritmo libre de derivadas con transformaciones de reflexión ($\alpha=1$), expansión ($\gamma=2$), contracción ($\rho=0.5$) y encogimiento ($\sigma=0.5$).
- **L-BFGS-B con Cotas de Caja (`minimize_lbfgs_b` / `optim(..., "L-BFGS-B", lower, upper)`):** Cálculo de gradiente proyectado sobre fronteras admisibles y proyección de parámetros $\text{clamp}(x_i, l_i, u_i)$.
- **Levenberg-Marquardt (`nls` / `nls_fit`):** Descenso Gauss-Newton amortiguado $(J^T J + \lambda \, \text{diag}(J^T J)) \Delta p = -J^T r$ para mínimos cuadrados no lineales y ajuste de curvas empíricas.
- **Máxima Verosimilitud con Inferencia Asintótica (`mle`):** Cálculo automático de la matriz de información de Fisher observada $\mathcal{I}(\hat{\theta})$, errores estándar asintóticos $\text{SE}$, estadísticos $z$, valores $p$, intervalos de confianza al 95% y criterios de información AIC / BIC.

---

#### 2. Suite de Integración y Pruebas
- **Pruebas en Runtime Rust (`crates/ghl-runtime/tests/eval_sem_estimator.rs`):**
  - `test_optim_bounded_lbfgs_b`: Validación de confinamiento de frontera en mínimo acotado con tolerancia $\epsilon < 10^{-3}$.
  - `test_nls_levenberg_marquardt`: Calibración de cinética enzimática de Michaelis-Menten con recuperación de parámetros $V_{\max}=10.0$ y $K_m=2.0$.
- **Pruebas en GHL (`packages/ghl_optim/tests/optim_test.gh`):**
  - 5/5 bloques de prueba superados: BFGS cuadrático, Nelder-Mead en Rosenbrock, L-BFGS-B acotado, NLS kinetics ($R^2 > 0.99$), y Gaussian MLE con inferencia asintótica.

---

#### 3. Entregables Documentales y Cuadernos
- **Guía Técnica Exhaustiva:** [`packages/ghl_optim/GUIDE.md`](packages/ghl_optim/GUIDE.md) documentando formulaciones matemáticas, API reference, benchmarks vs `scipy.optimize` y R `stats::optim`/`minpack.lm`.
- **Cuaderno Reproducible Quarto:** [`examples/nonlinear_optimization_and_mle.qmd`](examples/nonlinear_optimization_and_mle.qmd).
- **Tarjetas Cockpit Deck:** Paneles de terminal de alta fidelidad `optim_cockpit`, `mle_cockpit` y `nls_cockpit` con estética `/ᐠ˵- ⩊ -˵マ ✧ CONVERGED`.
