//! Migración del bloque "sigue boxeado" (Fase 3, commit `36ab513`) — mide el cruce real
//! secuencial-vs-paralelo específico de `sort_vector` (`sort_asc`/`sort_desc`), sin
//! reusar a ciegas `PARALLEL_THRESHOLD` (50.000, medido para una suma elemento-a-elemento
//! O(n)): un sort es O(n log n) con más trabajo por elemento (comparación + swap), así
//! que el cruce real puede estar en otro punto.
//!
//! Uso: `cargo run --release --example spike_sort_threshold_latency -p ghl-runtime`

use std::time::{Duration, Instant};
use rayon::prelude::*;

const REPEATS: u32 = 10;

fn best_of<T>(f: impl Fn() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut last = None;
    for _ in 0..REPEATS {
        let start = Instant::now();
        let result = f();
        best = best.min(start.elapsed());
        last = Some(result);
    }
    (best, last.unwrap())
}

fn run_size(n: usize) {
    println!("\nN = {n} elementos");
    let base: Vec<f64> = (0..n).map(|i| ((i * 2654435761) % 1_000_003) as f64).collect();

    let (t_seq, seq) = best_of(|| {
        let mut idx: Vec<u32> = (0..n as u32).collect();
        idx.sort_unstable_by(|&a, &b| base[a as usize].total_cmp(&base[b as usize]));
        idx
    });
    println!("  secuencial (sort_unstable_by):    min sobre {REPEATS}: {t_seq:>10.3?}");

    let (t_par, par) = best_of(|| {
        let mut idx: Vec<u32> = (0..n as u32).collect();
        idx.par_sort_unstable_by(|&a, &b| base[a as usize].total_cmp(&base[b as usize]));
        idx
    });
    println!("  paralelo (par_sort_unstable_by):  min sobre {REPEATS}: {t_par:>10.3?}");

    assert_eq!(seq.len(), par.len());
    for i in 0..seq.len() {
        assert_eq!(base[seq[i] as usize], base[par[i] as usize], "los dos deben dar el mismo orden");
    }

    let speedup = t_seq.as_secs_f64() / t_par.as_secs_f64().max(1e-12);
    println!("  paralelo vs secuencial: ~{speedup:.2}x");
}

fn main() {
    println!("hilos disponibles para rayon: {}", rayon::current_num_threads());
    for n in [1_000usize, 2_000, 3_000, 4_000, 5_000, 10_000, 20_000, 50_000, 1_000_000] {
        run_size(n);
    }
}
