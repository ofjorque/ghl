# Referencia de API — examples

> Generado automáticamente por `ghl doc` (Gojo & Haru Documentation Deck).

**Estadísticas de Cobertura:** 3/3 ítems documentados (100.0%)

## Índice de Símbolos

- [`fib()`](#fib) (function)
- [`compute_kinetic_energy()`](#compute_kinetic_energy) (function)
- [`solve_ols()`](#solve_ols) (function)

---

## Declaraciones

### `fib` (function)

```ghl
fn fib(n)
```

Compute the n-th Fibonacci number using recursive definition.

$$
F_n = F_{n-1} + F_{n-2}
$$

**Parámetros:**

- `n`: Integer position in the Fibonacci sequence

**Retorna:** `n-th Fibonacci number`

**Ejemplo de Código:**

```ghl
let res = fib(10);
```

---

### `compute_kinetic_energy` (function)

```ghl
fn compute_kinetic_energy(mass: f64, velocity: f64) -> f64
```

Calculate classical kinetic energy of a body given mass and velocity.

$$
E_k = (1/2) m v^2
$$

**Parámetros:**

- `mass`: Mass of the object in kilograms (kg)
- `velocity`: Velocity of the object in meters per second (m/s)

**Retorna:** `Kinetic energy in Joules (J)`

**Ejemplo de Código:**

```ghl
let energy = compute_kinetic_energy(2.0, 3.0);
```

---

### `solve_ols` (function)

```ghl
fn solve_ols(x: Matrix[f64], y: Vector[f64]) -> Vector[f64]
```

Solve ordinary least squares regression via the first-class backslash operator.

$$
β̂ = X \ y
$$

**Parámetros:**

- `x`: Design matrix of regressors
- `y`: Response vector of observations

**Retorna:** `Vector of estimated parameter coefficients`

**Ejemplo de Código:**

```ghl
let beta = solve_ols(X, y);
```

---

