//! Fase 4, punto (c) (Caso 3.3, Suite 03) — mide `bootstrap_mean` secuencial (un `for`
//! plano en vez de `into_par_iter()`, mismo cómputo exacto) contra la versión paralela con
//! rayon. A diferencia de los dos spikes anteriores de esta fase, este diseño no
//! materializa ningún buffer O(n) por réplica -- cada réplica es un acumulador `f64`
//! suelto sobre el mismo `&[f64]` base compartido -- así que el riesgo de OOM es bajo por
//! diseño, no por casualidad: memoria total ~O(n + n_replicas), no O(n * n_replicas).
//!
//! El enunciado completo del Caso 3.3 pide 20.000 réplicas × N=100.000 (2×10⁹ sorteos
//! aleatorios en total) -- eso tarda bastante incluso en release, así que el default acá es
//! más chico (2.000 × 100.000) y se extrapola linealmente para reportar una estimación de
//! la escala completa sin correrla de verdad.
//!
//! Uso: `cargo run --release --example spike_bootstrap_mean_latency -p ghl-runtime -- <n> <n_replicas>`

use std::time::Instant;
use rand::RngExt;
use rayon::prelude::*;

fn sequential_bootstrap_means(base: &[f64], n_replicas: usize) -> Vec<f64> {
    let n = base.len();
    (0..n_replicas)
        .map(|_| {
            let mut rng = rand::rng();
            let mut acc = 0.0;
            for _ in 0..n {
                let idx: usize = rng.random_range(0..n);
                acc += base[idx];
            }
            acc / n as f64
        })
        .collect()
}

fn parallel_bootstrap_means(base: &[f64], n_replicas: usize) -> Vec<f64> {
    let n = base.len();
    (0..n_replicas)
        .into_par_iter()
        .map(|_| {
            let mut rng = rand::rng();
            let mut acc = 0.0;
            for _ in 0..n {
                let idx: usize = rng.random_range(0..n);
                acc += base[idx];
            }
            acc / n as f64
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let n_replicas: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2_000);

    println!("N (tamaño de la muestra base) = {n}");
    println!("n_replicas = {n_replicas}");
    println!("hilos disponibles para rayon: {}", rayon::current_num_threads());

    let base: Vec<f64> = (0..n).map(|i| i as f64).collect();

    let start = Instant::now();
    let seq = sequential_bootstrap_means(&base, n_replicas);
    let t_seq = start.elapsed();
    println!("secuencial: {t_seq:>10.3?}");

    let start = Instant::now();
    let par = parallel_bootstrap_means(&base, n_replicas);
    let t_par = start.elapsed();
    println!("paralelo:   {t_par:>10.3?}");

    assert_eq!(seq.len(), n_replicas);
    assert_eq!(par.len(), n_replicas);
    let true_mean = (n - 1) as f64 / 2.0;
    let seq_grand_mean: f64 = seq.iter().sum::<f64>() / seq.len() as f64;
    let par_grand_mean: f64 = par.iter().sum::<f64>() / par.len() as f64;
    println!("media real de la base: {true_mean:.3} | gran media secuencial: {seq_grand_mean:.3} | gran media paralela: {par_grand_mean:.3}");

    let speedup = t_seq.as_secs_f64() / t_par.as_secs_f64().max(1e-12);
    println!("\nSpeedup paralelo vs secuencial: ~{speedup:.2}x");

    let full_scale_replicas = 20_000usize;
    let full_scale_n = 100_000usize;
    let scale_factor = (full_scale_replicas as f64 / n_replicas as f64) * (full_scale_n as f64 / n as f64);
    let est_seq_full = t_seq.as_secs_f64() * scale_factor;
    let est_par_full = t_par.as_secs_f64() * scale_factor;
    println!(
        "\nExtrapolación lineal a escala completa del Caso 3.3 (20.000 réplicas x N=100.000, NO corrida de verdad):"
    );
    println!("  secuencial estimado: ~{est_seq_full:.1}s | paralelo estimado: ~{est_par_full:.1}s");
}
