//! Fase 4, punto (a) — mide `vector_elementwise_op` (usado por `.+`/`.-`/`.*`/`./` y, desde
//! el fix de Fase 3, los operadores planos `+`/`-`/`*`/`/` entre dos `Vector`s numéricos
//! sin NA) en tres variantes, a varios tamaños, para elegir un `THRESHOLD` real de "vale la
//! pena repartir entre hilos" en vez de adivinarlo:
//!
//!   1. boxed:    el loop original sobre `Vec<Value>` (via `Deref`, un `Value::F64` por
//!                celda), tal como estaba antes de este punto.
//!   2. seq_f64:  mismo cómputo pero sobre el `&[f64]` de `VectorData::as_f64_view()`
//!                (cero boxing), un solo hilo.
//!   3. par_f64:  igual que (2) pero repartido con `rayon`'s `par_iter().zip()`.
//!
//! Uso: `cargo run --release --example spike_vector_elementwise_parallel_latency -p ghl-runtime`

use std::time::{Duration, Instant};
use rayon::prelude::*;
use ghl_runtime::value::Value;
use ghl_runtime::vector_data::VectorData;

const REPEATS: u32 = 10;

fn build_boxed(n: usize) -> Vec<Value> {
    (0..n).map(|i| Value::F64(i as f64 * 0.5)).collect()
}

fn boxed_add(a: &[Value], b: &[Value]) -> Vec<Value> {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| Value::F64(x.as_f64().unwrap_or(0.0) + y.as_f64().unwrap_or(0.0)))
        .collect()
}

fn seq_f64_add(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b.iter()).map(|(&x, &y)| x + y).collect()
}

fn par_f64_add(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.par_iter().zip(b.par_iter()).map(|(&x, &y)| x + y).collect()
}

#[allow(unused_assignments)]
fn best_of<T>(f: impl Fn() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut last = None;
    for _ in 0..REPEATS {
        // Drop the previous repeat's result *before* building the next one -- for an
        // elementwise op the result is a full O(n) buffer (unlike dot()'s O(1) scalar),
        // so without this the old and new result buffers briefly coexist, needlessly
        // doubling peak memory on top of the two input buffers already held by the caller.
        last = None;
        let start = Instant::now();
        let result = f();
        best = best.min(start.elapsed());
        last = Some(result);
    }
    (best, last.unwrap())
}

fn run_size(n: usize) {
    println!("\nN = {n} elementos");

    let boxed_a = build_boxed(n);
    let boxed_b = build_boxed(n);
    let (t_boxed, r_boxed) = best_of(|| boxed_add(&boxed_a, &boxed_b));
    println!("  boxed (Vec<Value>):    min sobre {REPEATS}: {t_boxed:>10.3?}");

    let flat_a: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
    let flat_b: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
    let vd_a = VectorData::from_f64(flat_a);
    let vd_b = VectorData::from_f64(flat_b);
    let view_a = vd_a.as_f64_view().expect("no nulls, fast path");
    let view_b = vd_b.as_f64_view().expect("no nulls, fast path");
    let (a, b) = (view_a.as_slice(), view_b.as_slice());

    let (t_seq, r_seq) = best_of(|| seq_f64_add(a, b));
    println!("  seq_f64 (sin boxing):  min sobre {REPEATS}: {t_seq:>10.3?}");

    let (t_par, r_par) = best_of(|| par_f64_add(a, b));
    println!("  par_f64 (rayon):       min sobre {REPEATS}: {t_par:>10.3?}");

    assert_eq!(r_boxed[0].as_f64(), Some(r_seq[0]));
    assert_eq!(r_seq, r_par);

    let speedup_seq_vs_boxed = t_boxed.as_secs_f64() / t_seq.as_secs_f64().max(1e-12);
    let speedup_par_vs_boxed = t_boxed.as_secs_f64() / t_par.as_secs_f64().max(1e-12);
    let speedup_par_vs_seq = t_seq.as_secs_f64() / t_par.as_secs_f64().max(1e-12);
    println!(
        "  seq_f64 vs boxed: ~{speedup_seq_vs_boxed:.2}x | par_f64 vs boxed: ~{speedup_par_vs_boxed:.2}x | par_f64 vs seq_f64: ~{speedup_par_vs_seq:.2}x"
    );
}

fn main() {
    // `size_of::<Value>() == 160` bytes (medido con un chequeo aparte, no asumido) -- mucho
    // más que lo estimado a ojo en sesiones previas. A diferencia de `dot()` (input+input+
    // un f64 escalar), una op elementwise produce un vector de salida O(n) completo, así
    // que el camino "boxed" sostiene hasta 3-4 buffers de tamaño N simultáneos (dos inputs
    // + el resultado). Un primer intento con 20_000_000 terminó en un OOM-kill real
    // (confirmado por `journalctl -k`: `anon-rss:11563088kB`) en este sandbox de 15GB --
    // 5_000_000 ya deja ver el efecto con margen de sobra (~3.2GB de pico en el peor caso,
    // bien lejos del límite) sin repetir el problema.
    println!("size_of::<Value>() = {} bytes", std::mem::size_of::<Value>());
    println!("hilos disponibles para rayon: {}", rayon::current_num_threads());
    for n in [1_000usize, 10_000, 20_000, 30_000, 50_000, 75_000, 100_000, 1_000_000, 5_000_000] {
        run_size(n);
    }
}
