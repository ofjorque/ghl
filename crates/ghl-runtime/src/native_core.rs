//! RNG/distributions, `dot`/`map`/`col`, and the shared `vector_native_reduce` reduction helper.
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use ghl_diagnostics::{Diagnostic, RenderCaps};
use polars_core::prelude::*;
use rand::{RngExt, SeedableRng};
use rand::distr::Distribution;
use rayon::prelude::*;
use statrs::distribution::{Continuous, ContinuousCDF, Gamma, Normal};
use crate::eval::Interpreter;
use crate::polars_bridge;
use crate::value::Value;
use crate::vector_data::VectorData;

// Built-in Native Functions

pub(crate) fn native_rm(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    for arg in args {
        match arg {
            Value::String(name) => {
                interp.env.remove(&name);
            }
            Value::ColRef(name) => {
                interp.env.remove(&name);
            }
            other => {
                let s = format!("{}", other);
                interp.env.remove(&s);
            }
        }
    }
    Ok(Value::Unit)
}

pub(crate) fn native_sha256(args: Vec<Value>) -> Result<Value, Diagnostic> {
    use sha2::{Digest, Sha256};
    let text = match args.first() {
        Some(Value::String(s)) => s.clone(),
        Some(other) => format!("{}", other),
        None => return Err(Diagnostic::compute_error("C0201", "`sha256()` requires an argument")),
    };
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    Ok(Value::String(hash))
}

pub(crate) fn native_help(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let caps = RenderCaps::rich_terminal(72);
    if let Some(arg) = args.first() {
        let name = match arg {
            Value::String(s) => s.clone(),
            Value::ColRef(s) => s.clone(),
            other => format!("{}", other),
        };
        if let Some(doc) = crate::doc::lookup_doc(&name) {
            println!("{}\n", doc.render(&caps));
            return Ok(Value::Unit);
        } else {
            return Err(Diagnostic::compute_error(
                "C0204",
                format!("No documentation found for `{name}`. Type `help()` to list common functions."),
            ));
        }
    }

    println!("{} GHL Help System:", caps.haru("ฅ(•⩊ •マ"));
    println!("Type `help(\"mean\")` or `?mean` to see signature and mathematical formula.");
    println!("Common functions: mean, sum, var, std_dev, median, ols, fit_logistic, fit_gmm,");
    println!("                  summary, predict, residuals, dot, cholesky, qr, svd, eigen,");
    println!("                  filter, select, mutate, group_by, summarize, plot, rm\n");
    Ok(Value::Unit)
}

/// Shared native-reduce path for `mean`/`sum`/`min`/`max`/`median` (TODO.md Fase 3, Track
/// 2, Punto 2): reduces on `VectorData`'s underlying `Column` directly (Arrow-vectorized,
/// no `Vec<Value>` boxing) instead of the boxed loop the pre-Punto-2 versions used.
/// Kleene NA propagation (RFC 02 sect2.4) preserves the *specific* reason of the first NA
/// found (`VectorData::first_na`, an O(1) check in the common no-NA case) -- an explicit
/// decision documented in TODO.md, not an oversight: unlike `summarize()`'s per-group
/// aggregation, a single `Vector` has no "which group's reason wins" ambiguity to hide
/// behind, so there's no reason to throw the specific reason away here.
pub(crate) fn vector_native_reduce(vd: &crate::vector_data::VectorData, kind: &str) -> Result<Value, Diagnostic> {
    if let Some(na) = vd.first_na() {
        return Ok(na);
    }
    let column = vd.column();
    let reduce_err = |e: PolarsError| Diagnostic::compute_error("C0210", format!("`{kind}()` failed: {e}"));
    let scalar = match kind {
        "mean" => column.mean_reduce(),
        "sum" => column.sum_reduce(),
        "min" => column.min_reduce(),
        "max" => column.max_reduce(),
        "median" => column.median_reduce(),
        other => unreachable!("vector_native_reduce: unknown kind `{other}`"),
    }.map_err(reduce_err)?;
    Ok(polars_bridge::any_value_to_plain_value(scalar.value()))
}

pub(crate) fn native_mean(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`mean()` requires at least 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "mean".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.is_empty() {
                return Ok(Value::NA(Some("EmptyVector".into())));
            }
            vector_native_reduce(vd, "mean")
        }
        _ => Err(Diagnostic::compute_error(
            "C0202",
            format!("`mean()` expects a Vector, found `{}`", vec_val.type_name()),
        )),
    }
}

pub(crate) fn native_sum(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`sum()` requires 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "sum".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.is_empty() {
                // Matches the old boxed loop's `sum = 0.0; has_float = false` starting
                // state: an empty sum is the additive identity, not a missing value.
                return Ok(Value::I64(0));
            }
            vector_native_reduce(vd, "sum")
        }
        _ => Err(Diagnostic::compute_error(
            "C0202",
            format!("`sum()` expects a Vector, found `{}`", vec_val.type_name()),
        )),
    }
}

pub(crate) fn native_var(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`var()` requires 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "var".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.len() < 2 {
                return Err(Diagnostic::statistical_warning(
                    "SW0002",
                    "Sample variance requires at least N=2 observations (N-1 degrees of freedom)",
                ));
            }
            if let Some(na) = vd.first_na() {
                return Ok(na);
            }
            let reduce_err = |e: PolarsError| Diagnostic::compute_error("C0210", format!("`var()` failed: {e}"));
            let scalar = vd.column().var_reduce(1).map_err(reduce_err)?;
            Ok(polars_bridge::any_value_to_plain_value(scalar.value()))
        }
        _ => Err(Diagnostic::compute_error(
            "C0202",
            format!("`var()` expects a Vector, found `{}`", vec_val.type_name()),
        )),
    }
}

pub(crate) fn native_std_dev(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if let Some(Value::ColRef(name)) = args.first() {
        return Ok(Value::AggSpec { kind: "std_dev".into(), col: Some(name.clone()) });
    }
    let var_val = native_var(args)?;
    if let Value::F64(v) = var_val {
        Ok(Value::F64(v.sqrt()))
    } else {
        Ok(var_val)
    }
}

pub(crate) fn native_min(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`min()` requires 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "min".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.is_empty() {
                return Ok(Value::NA(None));
            }
            vector_native_reduce(vd, "min")
        }
        _ => Err(Diagnostic::compute_error("C0202", "`min()` expects a Vector")),
    }
}

pub(crate) fn native_max(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`max()` requires 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "max".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.is_empty() {
                return Ok(Value::NA(None));
            }
            vector_native_reduce(vd, "max")
        }
        _ => Err(Diagnostic::compute_error("C0202", "`max()` expects a Vector")),
    }
}

/// `dot(a, b)` — dot product of two equal-length numeric `Vector`s (TODO.md Fase 3, Track
/// 2, Punto 2, Caso 1.1). Real SIMD via `MatrixOps::dot` (faer's blocked GEMM kernel via
/// `RowRef * ColRef`), not a hand-rolled loop -- `VectorData::as_f64_view` gets to that
/// kernel's input without boxing through `Vec<Value>` either way (zero-copy when both
/// vectors are already `Float64`, single-chunk, no nulls; a cast/rechunk otherwise, still
/// far cheaper than per-cell `Value` boxing). Kleene NA propagation: a NA anywhere in
/// either vector makes the whole dot product NA, preserving that cell's specific reason
/// (`VectorData::first_na`), same rationale as `mean()`/`sum()`/etc. above.
///
/// TODO.md Fase 5, Punto 1: seeds a `Xoshiro256PlusPlus` (already in `ghl-runtime`'s
/// dependencies since Fase 0, unused until now) from an explicit `i64` -- the "PRNG" here
/// is a plain value passed as an argument, not a persistent stateful object GHL scripts
/// hold onto and mutate (`PRNG::seed(seed)`'s aspirational style in Suite 03's own doc):
/// GHL has no variable reassignment (`is_mut` exists on `StmtKind::Let` but nothing ever
/// reads it -- there's no `ExprKind::Assign` at all) and no tuples/destructuring for a
/// `let (val, rng2) = draw(rng)` style either, so a mutating-handle PRNG object isn't
/// expressible today without a much bigger language change than "add reproducibility".
/// An explicit per-call seed sidesteps that entirely, and is naturally
/// parallel-friendly (see `native_bootstrap_mean` below) since nothing is shared/mutated
/// across calls.
pub(crate) fn seeded_rng(seed: i64) -> rand_xoshiro::Xoshiro256PlusPlus {
    rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(seed as u64)
}

/// `random_uniform(n)` — a `Vector[f64]` of `n` values drawn uniformly from `[0, 1)`
/// (TODO.md Fase 3, Track 2, Punto 4, Suite 01's own reference examples). Built straight
/// via `VectorData::from_f64`, no boxing, matching every other numeric-fast-path
/// constructor from Punto 1-3. Optional second argument `random_uniform(n, seed)` (same
/// optional-arity pattern as `round(v, digits)`) makes it bit-for-bit reproducible: same
/// `seed` and `n` always produce the same `Vector`, in any run. Without a `seed`, still
/// `rand::rng()` (thread-local, OS-seeded) -- not reproducible, unchanged from before.
pub(crate) fn native_random_uniform(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_uniform()` requires an integer length argument")
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`random_uniform()` length must be non-negative, found {n}"),
        ));
    }
    let data: Vec<f64> = match args.get(1).and_then(|v| v.as_i64()) {
        Some(seed) => {
            let mut rng = seeded_rng(seed);
            (0..n).map(|_| rng.random::<f64>()).collect()
        }
        None => {
            let mut rng = rand::rng();
            (0..n).map(|_| rng.random::<f64>()).collect()
        }
    };
    Ok(Value::Vector(VectorData::from_f64(data)))
}

/// Elements per chunk when parallelizing a seeded, per-element distribution sample (Fase
/// 5, Punto 2). Deliberately *not* one `Xoshiro256PlusPlus::jump()` per output element
/// (unlike `bootstrap_mean`, where each replica already does O(base_len) internal work,
/// making an O(n_replicas) sequential jump-setup negligible): here each element is a
/// single scalar draw, so an O(n) sequential setup before an O(n) parallel body would
/// scale badly and cap the achievable speedup as n grows (Amdahl's law) -- an
/// unavoidable trade-off of the same technique, not a hypothetical. Chunking to 1024
/// elements per jump cuts the sequential setup to O(n/1024) while staying far below
/// `PARALLEL_THRESHOLD` (50,000) itself, so there are always several chunks per thread
/// for rayon to balance.
const RNG_CHUNK_SIZE: usize = 1024;

/// Shared by `random_normal`/`random_gamma`: draws `n` i.i.d. samples from `dist`,
/// dispatching on size (`PARALLEL_THRESHOLD`, Fase 4 punto (a)) and on whether `seed` is
/// `Some` exactly like `random_uniform`/`bootstrap_mean` already do. The seeded+parallel
/// path assigns each *chunk* (not each element) its own `Xoshiro256PlusPlus` stream via
/// `.jump()` before the parallel `par_chunks_mut` loop starts, so which chunk lands on
/// which thread never affects the output -- same reproducibility guarantee as
/// `bootstrap_mean`, just chunked instead of per-replica.
pub(crate) fn sample_distribution<D: Distribution<f64> + Sync>(n: usize, dist: &D, seed: Option<i64>) -> Vec<f64> {
    match seed {
        Some(seed) => {
            if n < crate::eval::PARALLEL_THRESHOLD {
                let mut rng = seeded_rng(seed);
                return (0..n).map(|_| dist.sample(&mut rng)).collect();
            }
            let num_chunks = n.div_ceil(RNG_CHUNK_SIZE);
            let mut cursor = seeded_rng(seed);
            let mut chunk_rngs = Vec::with_capacity(num_chunks);
            for _ in 0..num_chunks {
                chunk_rngs.push(cursor.clone());
                cursor.jump();
            }
            let mut out = vec![0.0f64; n];
            out.par_chunks_mut(RNG_CHUNK_SIZE)
                .zip(chunk_rngs.into_par_iter())
                .for_each(|(chunk, mut rng)| {
                    for slot in chunk.iter_mut() {
                        *slot = dist.sample(&mut rng);
                    }
                });
            out
        }
        None => {
            if n < crate::eval::PARALLEL_THRESHOLD {
                let mut rng = rand::rng();
                return (0..n).map(|_| dist.sample(&mut rng)).collect();
            }
            let mut out = vec![0.0f64; n];
            out.par_chunks_mut(RNG_CHUNK_SIZE).for_each(|chunk| {
                let mut rng = rand::rng();
                for slot in chunk.iter_mut() {
                    *slot = dist.sample(&mut rng);
                }
            });
            out
        }
    }
}

/// `random_normal(n, mean, sd)` — a `Vector[f64]` of `n` samples from `Normal(mean, sd)`
/// (TODO.md Fase 5, Punto 2). Optional fourth argument `random_normal(n, mean, sd, seed)`
/// for bit-for-bit reproducibility, same convention as `random_uniform`/`bootstrap_mean`.
/// Backed by `statrs::distribution::Normal` rather than `rand_distr`'s -- see
/// `native_random_gamma` for why the choice actually matters for `Gamma` (not just here,
/// where both crates agree on the parameterization); kept the same crate for both so
/// there's one dependency to reason about instead of two.
pub(crate) fn native_random_normal(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_normal()` requires an integer length as its first argument")
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error("C0201", format!("`random_normal()` length must be non-negative, found {n}")));
    }
    let mean = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_normal()` second argument (mean) must be numeric")
    })?;
    let sd = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_normal()` third argument (sd) must be numeric")
    })?;
    let dist = Normal::new(mean, sd).map_err(|e| Diagnostic::compute_error("C0201", format!("`random_normal()`: {e}")))?;
    let seed = args.get(3).and_then(|v| v.as_i64());
    let data = sample_distribution(n as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}

/// `random_gamma(n, shape, rate)` — a `Vector[f64]` of `n` samples from `Gamma(shape,
/// rate)`, **shape-rate** parameterization (`rate = 1/scale`), matching
/// `statrs::distribution::Gamma::new` -- deliberately *not* shape-scale
/// (`rand_distr::Gamma`'s convention). This isn't cosmetic: Suite 03's own Gibbs sampler
/// reference computes `Gamma::new(1.0 + n/2.0, 1.0 + ssq/2.0)` to update a precision --
/// the standard Normal-Gamma Bayesian conjugate update, which is shape-rate. Naming the
/// GHL parameter `rate` (not `scale`) keeps this unambiguous; passing a scale value here
/// by mistake would silently produce a statistically different distribution, not an
/// error.
pub(crate) fn native_random_gamma(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_gamma()` requires an integer length as its first argument")
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error("C0201", format!("`random_gamma()` length must be non-negative, found {n}")));
    }
    let shape = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_gamma()` second argument (shape) must be numeric")
    })?;
    let rate = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_gamma()` third argument (rate) must be numeric")
    })?;
    let dist = Gamma::new(shape, rate).map_err(|e| Diagnostic::compute_error("C0201", format!("`random_gamma()`: {e}")))?;
    let seed = args.get(3).and_then(|v| v.as_i64());
    let data = sample_distribution(n as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}

/// `normal_pdf(x, mean, sd)` / `normal_cdf(x, mean, sd)` -- reuse the same
/// `statrs::distribution::Normal` already instantiated for sampling, just calling
/// `.pdf(x)`/`.cdf(x)` (its `Continuous`/`ContinuousCDF` impls) instead of `.sample(...)`.
/// `x` is a scalar, same treatment as `mean`/`sd` -- no broadcasting over a `Vector` in
/// this pass.
pub(crate) fn native_normal_pdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_pdf()` requires a numeric first argument (x)")
    })?;
    let mean = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_pdf()` second argument (mean) must be numeric")
    })?;
    let sd = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_pdf()` third argument (sd) must be numeric")
    })?;
    let dist = Normal::new(mean, sd).map_err(|e| Diagnostic::compute_error("C0201", format!("`normal_pdf()`: {e}")))?;
    Ok(Value::F64(dist.pdf(x)))
}

pub(crate) fn native_normal_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_cdf()` requires a numeric first argument (x)")
    })?;
    let mean = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_cdf()` second argument (mean) must be numeric")
    })?;
    let sd = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_cdf()` third argument (sd) must be numeric")
    })?;
    let dist = Normal::new(mean, sd).map_err(|e| Diagnostic::compute_error("C0201", format!("`normal_cdf()`: {e}")))?;
    Ok(Value::F64(dist.cdf(x)))
}

/// `gamma_pdf(x, shape, rate)` / `gamma_cdf(x, shape, rate)` -- same shape-rate
/// parameterization as `random_gamma()`, see its doc comment.
pub(crate) fn native_gamma_pdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_pdf()` requires a numeric first argument (x)")
    })?;
    let shape = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_pdf()` second argument (shape) must be numeric")
    })?;
    let rate = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_pdf()` third argument (rate) must be numeric")
    })?;
    let dist = Gamma::new(shape, rate).map_err(|e| Diagnostic::compute_error("C0201", format!("`gamma_pdf()`: {e}")))?;
    Ok(Value::F64(dist.pdf(x)))
}

pub(crate) fn native_gamma_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_cdf()` requires a numeric first argument (x)")
    })?;
    let shape = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_cdf()` second argument (shape) must be numeric")
    })?;
    let rate = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_cdf()` third argument (rate) must be numeric")
    })?;
    let dist = Gamma::new(shape, rate).map_err(|e| Diagnostic::compute_error("C0201", format!("`gamma_cdf()`: {e}")))?;
    Ok(Value::F64(dist.cdf(x)))
}

/// `bootstrap_mean(v, n_replicas)` — a `Vector[f64]` of `n_replicas` bootstrap sample
/// means, each computed by resampling `v` with replacement (TODO.md Fase 4, punto (c),
/// Suite 03's Caso 3.3). Deliberately just the mean, not an arbitrary user-supplied
/// estimator: a closure (like `map()`'s) captures `RuntimeEnv` mutably and swaps it into
/// the interpreter per call, which isn't `Send`/`Sync`-safe to hand to rayon without
/// redesigning that mechanism -- the mean, on the other hand, needs nothing but a scalar
/// accumulator per replica, so it parallelizes cleanly as-is.
///
/// "Shared memory without duplicating the base sample" (the case's own wording) comes
/// for free from `VectorData::as_f64_view()` (Punto 2, Fase 3): `base` below is a single
/// borrowed `&[f64]`, taken once, shared read-only across every rayon task -- no replica
/// ever copies the base sample, and no replica materializes its resampled subset either
/// (only a running `f64` sum), so memory use stays O(n + n_replicas), not O(n *
/// n_replicas).
///
/// TODO.md Fase 5, Punto 1: optional fourth-turned-third argument `bootstrap_mean(v,
/// n_replicas, seed)` makes this bit-for-bit reproducible -- and, since replicas run in
/// parallel, *independent of thread count/scheduling* too, which a naive `rand::rng()`
/// swap alone wouldn't give: two runs of the same seeded call could still assign
/// different random streams to different replicas depending on how rayon happens to
/// schedule them. Fixed by assigning each replica its own generator via
/// `Xoshiro256PlusPlus::jump()` *before* the parallel loop starts -- `.jump()` advances
/// the state by the equivalent of 2^128 draws, producing statistically independent,
/// non-overlapping streams (the standard way to parallelize this generator family;
/// naively combining `seed + replica_index` risks correlating neighboring streams
/// instead). Replica `i`'s stream is fixed by `i` alone, not by which thread happens to
/// run it, so 1 thread and 16 threads produce the same `Vector` of means. Without a
/// `seed`, unchanged: each task calls `rand::rng()` same as before.
pub(crate) fn native_bootstrap_mean(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = match args.first() {
        Some(Value::Vector(vd)) => vd,
        Some(other) => {
            return Err(Diagnostic::compute_error(
                "C0202",
                format!("`bootstrap_mean()` expects a Vector as its first argument, found `{}`", other.type_name()),
            ));
        }
        None => return Err(Diagnostic::compute_error("C0201", "`bootstrap_mean()` requires 2 arguments")),
    };
    let n_replicas = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`bootstrap_mean()` requires an integer replica count as its second argument")
    })?;
    if n_replicas < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`bootstrap_mean()` replica count must be non-negative, found {n_replicas}"),
        ));
    }
    if let Some(na) = vd.first_na() {
        // Same collapse-to-NA Kleene convention as dot()/mean(): any NA in the base sample
        // makes the estimator undefined, rather than silently excluding it from the
        // resampling pool (which would change the effective sample size).
        return Ok(na);
    }
    let view = vd.as_f64_view()?;
    let base = view.as_slice();
    let n = base.len();
    if n == 0 {
        return Err(Diagnostic::statistical_error("S0412", "`bootstrap_mean()` requires a non-empty Vector"));
    }

    let n_replicas = n_replicas as usize;
    let means: Vec<f64> = match args.get(2).and_then(|v| v.as_i64()) {
        Some(seed) => {
            // Assign each replica an independent, non-overlapping stream *before*
            // handing them to rayon -- see the doc comment above for why this (rather
            // than a shared/mutated generator, or combining seed+index by hand) is what
            // makes the result independent of thread count and scheduling.
            let mut cursor = seeded_rng(seed);
            let mut sub_rngs = Vec::with_capacity(n_replicas);
            for _ in 0..n_replicas {
                sub_rngs.push(cursor.clone());
                cursor.jump();
            }
            sub_rngs
                .into_par_iter()
                .map(|mut rng| {
                    let mut acc = 0.0;
                    for _ in 0..n {
                        let idx: usize = rng.random_range(0..n);
                        acc += base[idx];
                    }
                    acc / n as f64
                })
                .collect()
        }
        None => (0..n_replicas)
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
            .collect(),
    };

    Ok(Value::Vector(VectorData::from_f64(means)))
}

pub(crate) fn native_dot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let a = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`dot()` requires 2 arguments"))?;
    let b = args.get(1).ok_or_else(|| Diagnostic::compute_error("C0201", "`dot()` requires 2 arguments"))?;

    match (a, b) {
        (Value::Vector(va), Value::Vector(vb)) => {
            if va.len() != vb.len() {
                return Err(Diagnostic::statistical_error(
                    "S0412",
                    format!("`dot()`: vectors have different lengths ({} vs {})", va.len(), vb.len()),
                ));
            }
            if let Some(na) = va.first_na().or_else(|| vb.first_na()) {
                return Ok(na);
            }
            let view_a = va.as_f64_view()?;
            let view_b = vb.as_f64_view()?;
            let result = crate::matrix::MatrixOps::dot(view_a.as_slice(), view_b.as_slice())?;
            Ok(Value::F64(result))
        }
        (l, r) => Err(Diagnostic::compute_error(
            "C0202",
            format!("`dot()` expects two Vectors, found `{}` and `{}`", l.type_name(), r.type_name()),
        )),
    }
}

/// `map(x, f)` — applies `f` to every element of `Vector` `x` in a single pass (TODO.md
/// Fase 3, Track 2, Punto 3, Caso 1.4). This is a `NativeFnCtx` (not a plain `NativeFn`)
/// because it has to *call* `f` -- a `Value::Closure` or `Value::NativeFn` -- once per
/// element, and the only thing that knows how to invoke a `Closure` is
/// `Interpreter::call_value`.
///
/// The `Closure` branch deliberately does **not** call `call_value` in a loop: that
/// method's `Closure` arm consumes the closure's captured `RuntimeEnv` on every call
/// (`push_scope`, run the body, discard it), so calling it once per element would clone
/// the whole captured environment once per element too -- a real, avoidable cost, not
/// just overhead. Instead the environment is cloned and `push_scope`'d exactly once
/// before the loop, and each iteration only rebinds the one parameter (`RuntimeEnv::set`,
/// a `HashMap` insert) and does two `mem::replace` swaps (pointer/struct-field moves, not
/// clones) around a single `eval_expr` call.
///
/// The result is collected into one `Vec<Value>` (via `VectorData::from_values`), not a
/// flat `Vec<f64>`: `f` is arbitrary code and may legitimately produce `NA` for some
/// elements (e.g. `log(-1.0)` already does via `map_numeric_fn`), so the output isn't
/// guaranteed all-`f64` up front. The actual fusion this closes is avoiding one whole
/// intermediate `Vector` allocation *per sub-operation* of an expression like
/// `log(1.0 + exp(-abs(xi))) + sin(xi)` (six full-vector passes today if chained via the
/// existing vectorized helpers) -- not eliminating the one, unavoidable final boxing pass.
pub(crate) fn native_map(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = match args.first() {
        Some(Value::Vector(vd)) => vd.clone(),
        Some(other) => {
            return Err(Diagnostic::compute_error(
                "C0202",
                format!("`map()` expects a Vector as its first argument, found `{}`", other.type_name()),
            ));
        }
        None => return Err(Diagnostic::compute_error("C0201", "`map()` requires 2 arguments")),
    };
    let callable = args.get(1).cloned().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`map()` requires 2 arguments")
    })?;

    match callable {
        Value::NativeFn(func) => {
            let mut out = Vec::with_capacity(vd.len());
            for elem in vd.iter() {
                out.push(func(vec![elem.clone()])?);
            }
            Ok(Value::Vector(VectorData::from_values(out)))
        }
        Value::Closure { params, body, env: closure_env } => {
            let param_name = params.first().cloned().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`map()`'s function must take exactly 1 parameter")
            })?;

            // One-time setup -- see the doc comment above for why this isn't `call_value`
            // in a loop.
            let mut call_env = closure_env;
            if let Some(global_scope) = interp.env.scopes.first().cloned() {
                for (k, v) in global_scope {
                    if call_env.get(&k).is_none() {
                        call_env.set(k, v);
                    }
                }
            }
            call_env.push_scope();

            let mut out = Vec::with_capacity(vd.len());
            for elem in vd.iter() {
                call_env.set(param_name.clone(), elem.clone());
                let old_env = std::mem::replace(&mut interp.env, call_env);
                let result = interp.eval_expr(&body);
                call_env = std::mem::replace(&mut interp.env, old_env);
                out.push(result?);
            }
            Ok(Value::Vector(VectorData::from_values(out)))
        }
        other => Err(Diagnostic::compute_error(
            "C0203",
            format!("`map()`'s second argument must be callable, found `{}`", other.type_name()),
        )),
    }
}

pub(crate) fn native_col(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let col_name = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`col()` requires a string column name")
    })?;
    Ok(Value::ColRef(col_name.to_string()))
}
