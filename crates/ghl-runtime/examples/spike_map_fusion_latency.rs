//! Fase 3, Track 2, Punto 3 (Caso 1.4, Suite 01) — mide la expresión de referencia del
//! caso, `log(1.0 + exp(-abs(xi))) + sin(xi)`, por el único camino que existía antes de
//! `map()`: encadenar los helpers vectorizados existentes (`abs` -> `exp` -> suma escalar
//! -> `log` -> `sin` -> suma final), seis pasadas completas sobre el vector con un
//! `Vector` intermedio completo por cada una -- contra `map()` nuevo, una sola pasada.
//!
//! El vector de entrada se inyecta directo en el entorno del intérprete (`interp.env.set`)
//! en vez de escribirlo como un literal `.gh` de N elementos -- con N=5×10^7 un literal así
//! sería dominado por tiempo de *parseo* del texto fuente, no de ejecución, y ese no es lo
//! que este spike quiere medir.
//!
//! Nota honesta (ver TODO.md): la ganancia acá es por evitar los `Vector` intermedios por
//! sub-operación, no por acelerar la aritmética elemento a elemento -- esa sigue siendo
//! `Value` escalar interpretado en ambos caminos. No se espera (ni se fuerza) un número
//! tan grande como el ~14x de `dot()` (Punto 2, SIMD real reemplazando un loop boxeado).
//!
//! El default de N (2M, no los 5×10^7 que pide el enunciado del Caso 1.4 al pie de la
//! letra) es deliberado: el camino "encadenado" que este spike mide *a propósito* aloca
//! seis `Vec<Value>` completos a la vez (eso es justo el problema que `map()` resuelve),
//! y con 5×10^7 elementos eso implica varios GB simultáneos -- en este sandbox compartido
//! ya ajustado de memoria (ver TODO.md, el hallazgo de `debug = "line-tables-only"`) un
//! primer intento a 5×10^7 terminó en un OOM-kill real (confirmado por dmesg). 2M ya
//! deja ver el efecto con margen de sobra.
//!
//! Uso: `cargo run --release --example spike_map_fusion_latency -p ghl-runtime -- <n>`

use std::time::Instant;
use ghl_syntax::parser::parse;
use ghl_runtime::value::Value;
use ghl_runtime::vector_data::VectorData;
use ghl_runtime::Interpreter;

const REPEATS: u32 = 5;

fn run(code: &str, base_vector: &VectorData) -> std::time::Duration {
    let program = parse(code).expect("syntax ok");
    let mut best = std::time::Duration::MAX;
    for _ in 0..REPEATS {
        let mut interp = Interpreter::new();
        // Clona el `VectorData` (barato, Arc por dentro -- Punto 1), no los datos crudos:
        // evita copiar el `Vec<f64>` base una vez por cada una de las `REPEATS * 2`
        // corridas, que era puro overhead de medición, no parte de lo que se quiere medir.
        interp.env.set("x".to_string(), Value::Vector(base_vector.clone()));
        let start = Instant::now();
        interp.eval_program(&program).expect("evaluation ok");
        best = best.min(start.elapsed());
    }
    best
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2_000_000);

    println!("N = {n} elementos");

    let xs: Vec<f64> = (0..n).map(|i| (i as f64 * 0.001).sin() * 3.0).collect();
    let base_vector = VectorData::from_f64(xs);

    // Dos huecos preexistentes de la aritmética de Vector, ninguno en el alcance de este
    // punto (TODO.md ya los tiene anotados): `1.0 + b` (escalar + Vector) no está
    // soportado, solo el orden `Vector op escalar` (`b + 1.0` sí); y `d + e` (Vector +
    // Vector con `+` liso) tampoco -- hace falta el operador elemento-a-elemento `.+`.
    let chained = r#"
        let a = abs(x);
        let b = exp(a);
        let c = b + 1.0;
        let d = log(c);
        let e = sin(x);
        let y = d .+ e;
    "#;
    let fused = r#"
        let y = map(x, \xi -> log(1.0 + exp(-abs(xi))) + sin(xi));
    "#;

    let best_chained = run(chained, &base_vector);
    println!("  Encadenado (6 Vector completos, camino viejo): min sobre {REPEATS} corridas: {best_chained:>10.3?}");

    let best_fused = run(fused, &base_vector);
    println!("  map() fusionado (1 pasada, camino nuevo):      min sobre {REPEATS} corridas: {best_fused:>10.3?}");

    if !best_fused.is_zero() {
        let speedup = best_chained.as_secs_f64() / best_fused.as_secs_f64();
        println!("\nSpeedup: ~{speedup:.2}x");
    }
}
