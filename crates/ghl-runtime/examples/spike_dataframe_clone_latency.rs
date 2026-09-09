//! Fase 1 — mide el "Copy-on-Write" acotado a `Value::DataFrame`/`GroupedDataFrame`
//! (TODO.md, Caso 2.3): `na_reasons` pasó de `NaReasonTable` (dueño) a
//! `Arc<NaReasonTable>`, así que clonar un `Value::DataFrame` -- lo que pasa en cada
//! lookup de variable, cada binding, cada paso por valor -- ya no clona la tabla de
//! razones entera. Este spike mide el costo real que eso evitaba: clonar la misma tabla
//! (con N celdas `NA:razon` registradas, el peor caso) por el camino viejo (clonar el
//! `NaReasonTable` completo) contra el nuevo (clonar el `Arc`).
//!
//! Uso: `cargo run --release --example spike_dataframe_clone_latency -p ghl-runtime -- <n_reasons>`

use std::sync::Arc;
use std::time::Instant;
use ghl_runtime::na_reasons::NaReasonTable;

const REPEATS: u32 = 50;

fn build_table(n: usize) -> NaReasonTable {
    let mut t = NaReasonTable::new();
    for i in 0..n {
        t.set("score", i, format!("SensorDropout-{i}"));
    }
    t
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(200_000);

    let table = build_table(n);
    let arc_table = Arc::new(build_table(n));

    println!("NaReasonTable con {n} razones registradas (peor caso: todas las celdas)");

    let mut best_deep = std::time::Duration::MAX;
    for _ in 0..REPEATS {
        let start = Instant::now();
        let _cloned = table.clone();
        best_deep = best_deep.min(start.elapsed());
    }
    println!("  Clonar NaReasonTable completa (camino viejo): min sobre {REPEATS} corridas: {best_deep:>10.3?}");

    let mut best_arc = std::time::Duration::MAX;
    for _ in 0..REPEATS {
        let start = Instant::now();
        let _cloned = Arc::clone(&arc_table);
        best_arc = best_arc.min(start.elapsed());
    }
    println!("  Clonar Arc<NaReasonTable> (camino nuevo):    min sobre {REPEATS} corridas: {best_arc:>10.3?}");

    if !best_arc.is_zero() {
        let speedup = best_deep.as_secs_f64() / best_arc.as_secs_f64();
        println!("\nSpeedup: ~{speedup:.0}x (crece con N -- el camino nuevo es O(1), el viejo es O(N))");
    }
}
