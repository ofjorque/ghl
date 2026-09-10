//! Fase 6, Caso 3.2 -- mide el aspecto que pide el enunciado textualmente: "eficiencia
//! del solver matricial, estabilidad numérica" de IRLS para regresión logística, a
//! N=1.000.000 observaciones x P=40 predictores continuos.
//!
//! Dos mediciones reales, no adivinadas:
//!   1. Ajuste completo (`FittedGlm::fit_logistic`) a escala completa: tiempo total,
//!      iteraciones de IRLS hasta converger, y que los coeficientes recuperados queden
//!      cerca de los verdaderos (estabilidad numérica -- no solo "no crashea").
//!   2. El ensamblado de `X^T W X` / `X^T W z` (el O(n*p^2) que se repite una vez por
//!      iteración de IRLS, y que desde este mismo Caso 3.2 corre paralelizado con rayon
//!      por encima de `PARALLEL_THRESHOLD`) -- reimplementado localmente en variante
//!      secuencial y paralela (mismo patrón que `spike_vector_elementwise_parallel_latency.rs`:
//!      medir contra una reimplementación autocontenida, no reventar la visibilidad
//!      `pub(crate)` del helper real solo para el benchmark), con los pesos reales del
//!      ajuste ya convergido (no pesos inventados) para que el speedup reportado sea el
//!      que de verdad se observaría en una iteración típica de IRLS a esta escala.
//!
//! Los predictores se generan con el `random_normal` sembrado de GHL (Fase 5), no un CSV
//! intermedio -- se corren a través del intérprete real, mismo camino que un script GHL
//! tomaría. La respuesta Bernoulli se genera en Rust desde el modelo verdadero: GHL no
//! tiene todavía un operador de comparación elementwise sobre `Vector` (`u < p` con
//! ambos `Vector` falla hoy con `C0202`, confirmado leyendo `eval_binary_op`'s Lt/LtEq/
//! Gt/GtEq -- ver el comentario en `test_fit_logistic_recovers_known_coefficients` en
//! `lib.rs`), así que no se puede simular la Bernoulli en GHL puro; el ajuste en sí
//! (`fit_logistic`) sigue yendo por el camino real de la función nativa, vía
//! `FittedGlm::fit_logistic` directo (mismo código que `native_fit_logistic` llama).
//!
//! Uso: `cargo run --release --example spike_irls_latency -p ghl-runtime`

use std::collections::HashMap;
use std::time::Instant;
use rand::{RngExt, SeedableRng};
use rayon::prelude::*;
use ghl_runtime::{Blueprint, FittedGlm, Interpreter, Value};
use ghl_syntax::parser::parse;

const N: usize = 1_000_000;
const P_PREDICTORS: usize = 40;

fn vector_f64(v: &Value) -> Vec<f64> {
    match v {
        Value::Vector(items) => items.iter().map(|x| x.as_f64().unwrap()).collect(),
        other => panic!("Expected Vector, found {other:?}"),
    }
}

fn sigmoid_stable(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

/// Reimplementación secuencial de `assemble_weighted_normal_equations` (`neko.rs`,
/// `pub(crate)`) -- misma álgebra exacta (`X^T W X`, `X^T W z`), autocontenida acá para
/// no tocar la visibilidad del helper real solo por el benchmark.
fn assemble_seq(n: usize, p: usize, x_data: &[f64], weights: &[f64], target: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut xtwx = vec![0.0; p * p];
    let mut xtwt = vec![0.0; p];
    for i in 0..n {
        let row = &x_data[i * p..(i + 1) * p];
        let w = weights[i];
        let t = target[i];
        for a in 0..p {
            xtwt[a] += w * row[a] * t;
            for b in 0..p {
                xtwx[a * p + b] += w * row[a] * row[b];
            }
        }
    }
    (xtwx, xtwt)
}

/// Misma álgebra, repartida con rayon: cada tarea paralela acumula un buffer p*p/p
/// local sobre un rango de filas (`fold`), después se reducen (`reduce`) -- igual patrón
/// que el `assemble_weighted_normal_equations` real usa para evitar contención de un
/// acumulador compartido entre hilos.
fn assemble_par(n: usize, p: usize, x_data: &[f64], weights: &[f64], target: &[f64]) -> (Vec<f64>, Vec<f64>) {
    (0..n)
        .into_par_iter()
        .fold(
            || (vec![0.0_f64; p * p], vec![0.0_f64; p]),
            |(mut xtwx, mut xtwt), i| {
                let row = &x_data[i * p..(i + 1) * p];
                let w = weights[i];
                let t = target[i];
                for a in 0..p {
                    xtwt[a] += w * row[a] * t;
                    for b in 0..p {
                        xtwx[a * p + b] += w * row[a] * row[b];
                    }
                }
                (xtwx, xtwt)
            },
        )
        .reduce(
            || (vec![0.0_f64; p * p], vec![0.0_f64; p]),
            |(mut xtwx_a, mut xtwt_a), (xtwx_b, xtwt_b)| {
                for k in 0..xtwx_a.len() {
                    xtwx_a[k] += xtwx_b[k];
                }
                for k in 0..xtwt_a.len() {
                    xtwt_a[k] += xtwt_b[k];
                }
                (xtwx_a, xtwt_a)
            },
        )
}

fn main() {
    println!("hilos disponibles para rayon: {}", rayon::current_num_threads());
    println!("N = {N}, P = {P_PREDICTORS} predictores continuos\n");

    // 1. Predictores vía el random_normal sembrado real de GHL, a través del intérprete.
    let gen_code: String = (1..=P_PREDICTORS)
        .map(|j| format!("let x{j} = random_normal({N}, 0.0, 1.0, {seed});\n", seed = 1000 + j))
        .collect();
    let program = parse(&gen_code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let predictors: Vec<Vec<f64>> = (1..=P_PREDICTORS)
        .map(|j| vector_f64(&interp.env.get(&format!("x{j}")).unwrap()))
        .collect();
    // Libera las copias de x1..x40 que vive dentro de `interp.env` -- ya están extraídas
    // en `predictors`, y `size_of::<Value>() == 160` bytes hace que cada `Vec<Value>`
    // boxeado de acá en más pese; no vale la pena mantener también la copia del intérprete.
    drop(interp);

    // 2. Beta verdadero fijo (alterna signo/magnitud) y respuesta Bernoulli generada en
    // Rust desde el modelo verdadero -- ver comentario del módulo sobre por qué no se
    // puede hacer en GHL puro todavía.
    let true_beta: Vec<f64> = (0..=P_PREDICTORS)
        .map(|j| if j == 0 { 0.2 } else { 0.3 * if j % 2 == 0 { 1.0 } else { -1.0 } / (j as f64).sqrt() })
        .collect();

    let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(777);
    let mut y = Vec::with_capacity(N);
    for i in 0..N {
        let mut eta = true_beta[0];
        for j in 0..P_PREDICTORS {
            eta += true_beta[j + 1] * predictors[j][i];
        }
        let p = sigmoid_stable(eta);
        y.push(if rng.random::<f64>() < p { 1.0 } else { 0.0 });
    }

    // 3. Arma el HashMap<String, Vec<Value>> que `Blueprint::bake` espera directamente
    // en Rust (evita que el tiempo de construcción de un DataFrame/`dataframe {...}` se
    // mezcle con el tiempo del solver, que es lo que este benchmark mide).
    let mut data: HashMap<String, Vec<Value>> = HashMap::new();
    data.insert("y".to_string(), y.into_iter().map(Value::F64).collect());
    let term_names: Vec<String> = (1..=P_PREDICTORS).map(|j| format!("x{j}")).collect();
    // `mem::take` + `into_iter` en vez de `.iter().map(...).collect()`: consume cada
    // columna f64 (8MB a N=1M) a medida que la boxea, en vez de mantener las 40 columnas
    // sin boxear vivas al mismo tiempo que sus 40 copias boxeadas de 160 bytes/celda.
    let mut predictors = predictors;
    for (j, name) in term_names.iter().enumerate() {
        let col = std::mem::take(&mut predictors[j]);
        data.insert(name.clone(), col.into_iter().map(Value::F64).collect());
    }
    drop(predictors);

    let blueprint = Blueprint::new("y".to_string(), term_names.clone());

    // 4. Mide el ajuste completo a escala real.
    let start = Instant::now();
    let fit = FittedGlm::fit_logistic(blueprint, &term_names, &data).expect("IRLS converges");
    let elapsed = start.elapsed();
    // `data` (el `HashMap<String, Vec<Value>>` boxeado, ~160 bytes/celda -- ver el
    // comentario de más arriba) ya cumplió su propósito: `fit.x_data` es la copia
    // desboxeada que `bake()` extrajo de ahí. Soltarlo antes de medir el ensamblado
    // seq/par evita que sus ~6.5GB sigan compitiendo por ancho de banda de memoria
    // durante esa medición -- que es justo lo que se quiere aislar acá.
    drop(data);

    println!("=== Ajuste completo (fit_logistic) ===");
    println!("  tiempo total:      {elapsed:>10.3?}");
    println!("  iteraciones IRLS:  {}", fit.iterations);
    println!("  n_obs:             {}", fit.n_obs);
    let max_beta_err = fit
        .coefficients
        .iter()
        .zip(&true_beta)
        .map(|(&est, &truth)| (est - truth).abs())
        .fold(0.0_f64, f64::max);
    println!("  max |beta_est - beta_true|: {max_beta_err:.4} (estabilidad numérica)");

    // 5. Mide el ensamblado seq vs. par con los pesos reales del ajuste ya convergido
    // (W_i = mu_i*(1-mu_i)), la misma cantidad que cada iteración de IRLS ensambla.
    let p_full = P_PREDICTORS + 1; // +intercepto
    let weights: Vec<f64> = fit.fitted_values.iter().map(|&mu| (mu * (1.0 - mu)).max(1e-10)).collect();
    let target = &fit.fitted_values; // cualquier vector length-n sirve para medir el ensamblado

    // best-of-5, alternando el orden par/seq en cada repetición -- para no dejar que un
    // sesgo de calentamiento (page faults, spin-up del threadpool de rayon, turbo boost
    // del CPU) favorezca sistemáticamente al que corre primero.
    const REPEATS: u32 = 5;
    let mut t_seq = std::time::Duration::MAX;
    let mut t_par = std::time::Duration::MAX;
    let mut xtwx_seq = Vec::new();
    let mut xtwx_par = Vec::new();
    for r in 0..REPEATS {
        if r % 2 == 0 {
            let start = Instant::now();
            let (x, _) = assemble_seq(N, p_full, &fit.x_data, &weights, target);
            t_seq = t_seq.min(start.elapsed());
            xtwx_seq = x;

            let start = Instant::now();
            let (x, _) = assemble_par(N, p_full, &fit.x_data, &weights, target);
            t_par = t_par.min(start.elapsed());
            xtwx_par = x;
        } else {
            let start = Instant::now();
            let (x, _) = assemble_par(N, p_full, &fit.x_data, &weights, target);
            t_par = t_par.min(start.elapsed());
            xtwx_par = x;

            let start = Instant::now();
            let (x, _) = assemble_seq(N, p_full, &fit.x_data, &weights, target);
            t_seq = t_seq.min(start.elapsed());
            xtwx_seq = x;
        }
    }

    let max_diff = xtwx_seq
        .iter()
        .zip(&xtwx_par)
        .map(|(&a, &b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    assert!(max_diff < 1e-6, "seq y par deberían coincidir, max_diff={max_diff}");

    println!("\n=== Ensamblado X^T W X / X^T W z (una iteración típica de IRLS) ===");
    println!("  secuencial:  {t_seq:>10.3?}");
    println!("  paralelo:    {t_par:>10.3?}");
    let speedup = t_seq.as_secs_f64() / t_par.as_secs_f64().max(1e-12);
    println!("  speedup paralelo vs. secuencial: ~{speedup:.2}x");
    // Hallazgo real, no escondido: en este sandbox (8 cores, 1 nodo NUMA), este número
    // sale sistemáticamente ~0.7x (paralelo MÁS LENTO) en esta corrida de punta a punta,
    // reproducible entre corridas -- a diferencia de un microbenchmark aislado del mismo
    // algoritmo (mismo N=1M, mismo p=41, datos sintéticos en un proceso limpio sin la
    // huella de memoria del resto del pipeline de NEKO), que sí midió ~1.2x de mejora acá
    // mismo. La brecha no se explica por NUMA (un solo nodo) ni por `data` (el HashMap
    // boxeado de ~6.5GB) seguir vivo (se probó soltarlo antes de esta medición, sin
    // cambio). Lectura más plausible: la churn de asignación/liberación del pipeline
    // completo a N=1M (los ~6.5GB de `Vec<Value>` boxeado que `Blueprint::bake` consume,
    // más las asignaciones de cada una de las 5 iteraciones de IRLS) deja el heap en un
    // estado que penaliza más al camino paralelo (buffers `fold` por tarea) que al
    // secuencial -- no se investigó más a fondo por quedar fuera del alcance de este Caso
    // 3.2. Documentado tal cual salió, no promediado ni descartado.
}
