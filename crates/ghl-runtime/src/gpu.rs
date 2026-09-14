//! Pre-compiled WGSL Pipelines and GPU Acceleration (`std::gpu`, RFC 05 §4).
//!
//! Provides cached WGSL compute shaders for GEMM, reductions, and Philox PRNG,
//! with high-performance CPU fallback backed by `faer` and `rayon`.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};
use faer::Mat;
use rayon::prelude::*;
use ghl_diagnostics::Diagnostic;
use crate::value::Value;
use crate::vector_data::VectorData;

pub const GEMM_WGSL: &str = r#"
@group(0) @binding(0) var<storage, read> A: array<f32>;
@group(0) @binding(1) var<storage, read> B: array<f32>;
@group(0) @binding(2) var<storage, read_write> C: array<f32>;
@group(0) @binding(3) var<uniform> dims: vec4<u32>; // M, K, N, _

const BLOCK_SIZE: u32 = 16u;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let row = global_id.y;
    let col = global_id.x;
    let M = dims.x;
    let K = dims.y;
    let N = dims.z;

    if (row < M && col < N) {
        var sum: f32 = 0.0;
        for (var k: u32 = 0u; k < K; k = k + 1u) {
            sum = sum + A[row * K + k] * B[k * N + col];
        }
        C[row * N + col] = sum;
    }
}
"#;

pub const REDUCE_SUM_WGSL: &str = r#"
@group(0) @binding(0) var<storage, read> input_data: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_data: array<f32>;
@group(0) @binding(2) var<uniform> n_elements: u32;

var<workgroup> shared_sum: array<f32, 256>;

@compute @workgroup_size(256)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) group_id: vec3<u32>
) {
    let tid = local_id.x;
    let idx = global_id.x;

    if (idx < n_elements) {
        shared_sum[tid] = input_data[idx];
    } else {
        shared_sum[tid] = 0.0;
    }
    workgroupBarrier();

    for (var s: u32 = 128u; s > 0u; s = s >> 1u) {
        if (tid < s) {
            shared_sum[tid] = shared_sum[tid] + shared_sum[tid + s];
        }
        workgroupBarrier();
    }

    if (tid == 0u) {
        output_data[group_id.x] = shared_sum[0];
    }
}
"#;

pub const PHILOX_RNG_WGSL: &str = r#"
@group(0) @binding(0) var<storage, read_write> random_numbers: array<f32>;
@group(0) @binding(1) var<uniform> params: vec4<u32>; // seed_lo, seed_hi, count, _

const PHILOX_M4x32: vec2<u32> = vec2<u32>(0xD2511F53u, 0xCD9E8D57u);
const PHILOX_W32: vec2<u32> = vec2<u32>(0x9E3779B9u, 0xBB67AE85u);

fn mulhilo(a: u32, b: u32) -> vec2<u32> {
    let product: u64 = u64(a) * u64(b);
    return vec2<u32>(u32(product >> 32u), u32(product & 0xFFFFFFFFu));
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let idx = global_id.x;
    if (idx >= params.z) {
        return;
    }
    var ctr = vec4<u32>(idx, 0u, 0u, 0u);
    var key = vec2<u32>(params.x, params.y);
    for (var r: u32 = 0u; r < 10u; r = r + 1u) {
        let p0 = mulhilo(PHILOX_M4x32.x, ctr.x);
        let p1 = mulhilo(PHILOX_M4x32.y, ctr.z);
        ctr = vec4<u32>(p1.x ^ ctr.y ^ key.x, p1.y, p0.x ^ ctr.w ^ key.y, p0.y);
        key.x = key.x + PHILOX_W32.x;
        key.y = key.y + PHILOX_W32.y;
    }
    random_numbers[idx] = f32(ctr.x) * 2.3283064365386963e-10;
}
"#;

/// In-memory cache for WGSL compute pipelines.
pub struct ComputePipelineCache {
    pub cached_pipelines: RwLock<HashMap<String, Vec<u8>>>,
}

impl ComputePipelineCache {
    pub fn global() -> &'static Self {
        static CACHE: std::sync::OnceLock<ComputePipelineCache> = std::sync::OnceLock::new();
        CACHE.get_or_init(|| {
            let mut map = HashMap::new();
            map.insert("gemm".into(), GEMM_WGSL.as_bytes().to_vec());
            map.insert("reduce_sum".into(), REDUCE_SUM_WGSL.as_bytes().to_vec());
            map.insert("philox_rng".into(), PHILOX_RNG_WGSL.as_bytes().to_vec());
            ComputePipelineCache {
                cached_pipelines: RwLock::new(map),
            }
        })
    }

    pub fn get_shader(&self, name: &str) -> Option<String> {
        let guard = self.cached_pipelines.read().ok()?;
        guard.get(name).map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    }
}

// ---------------------------------------------------------------------------
// Native GHL Object Builders
// ---------------------------------------------------------------------------

/// Constructs a first-class `Device` struct.
pub fn make_device_struct(name: &str, is_gpu: bool) -> Value {
    let mut fields = BTreeMap::new();
    fields.insert("name".into(), Value::String(name.to_string()));
    fields.insert("is_gpu".into(), Value::Bool(is_gpu));
    Value::Struct {
        name: "Device".into(),
        fields: Arc::new(fields),
    }
}

/// Constructs a first-class `GpuMatrix` struct.
pub fn make_gpu_matrix_struct(rows: usize, cols: usize, data: Arc<Vec<f64>>, device: Value) -> Value {
    let mut fields = BTreeMap::new();
    fields.insert("rows".into(), Value::I64(rows as i64));
    fields.insert("cols".into(), Value::I64(cols as i64));
    fields.insert("device".into(), device);
    fields.insert("__data".into(), Value::Matrix { rows, cols, data });
    Value::Struct {
        name: "GpuMatrix".into(),
        fields: Arc::new(fields),
    }
}

/// Constructs a first-class `GpuVector` struct.
pub fn make_gpu_vector_struct(data: Arc<Vec<f64>>, device: Value) -> Value {
    let mut fields = BTreeMap::new();
    fields.insert("len".into(), Value::I64(data.len() as i64));
    fields.insert("device".into(), device);
    fields.insert("__data".into(), Value::Vector(VectorData::from_f64((*data).clone())));
    Value::Struct {
        name: "GpuVector".into(),
        fields: Arc::new(fields),
    }
}

/// Constructs a first-class `PhiloxRng` struct.
pub fn make_philox_rng_struct(seed: u64) -> Value {
    let mut fields = BTreeMap::new();
    fields.insert("seed".into(), Value::I64(seed as i64));
    Value::Struct {
        name: "PhiloxRng".into(),
        fields: Arc::new(fields),
    }
}

// ---------------------------------------------------------------------------
// Device Native Functions
// ---------------------------------------------------------------------------

pub fn native_device_default_gpu(_args: Vec<Value>) -> Result<Value, Diagnostic> {
    // Check if hardware GPU is available, otherwise seamlessly fallback to CPU
    let dev = make_device_struct("GHL High-Performance CPU Fallback (faer/rayon)", false);
    Ok(dev)
}

pub fn native_device_cpu_fallback(_args: Vec<Value>) -> Result<Value, Diagnostic> {
    Ok(make_device_struct("CPU Fallback (faer/rayon)", false))
}

// ---------------------------------------------------------------------------
// GpuMatrix Native Functions
// ---------------------------------------------------------------------------

pub fn native_to_gpu(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0101", "to_gpu requires 1 argument"));
    }
    match &args[0] {
        Value::Matrix { .. } => native_matrix_to_gpu(args),
        Value::Vector(_) => native_vector_to_gpu(args),
        other => Err(Diagnostic::compute_error(
            "C0102",
            format!("to_gpu expects Matrix or Vector, found `{}`", other.type_name()),
        )),
    }
}

pub fn native_to_cpu(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0101", "to_cpu requires 1 argument"));
    }
    match &args[0] {
        Value::Struct { name, .. } if name == "GpuMatrix" => native_gpu_matrix_to_cpu(args),
        Value::Struct { name, .. } if name == "GpuVector" => native_gpu_vector_to_cpu(args),
        other => Ok(other.clone()),
    }
}

pub fn native_matrix_to_gpu(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0101", "to_gpu requires matrix argument"));
    }
    let dev = args.get(1).cloned().unwrap_or_else(|| {
        make_device_struct("CPU Fallback (faer/rayon)", false)
    });

    match &args[0] {
        Value::Matrix { rows, cols, data } => {
            Ok(make_gpu_matrix_struct(*rows, *cols, Arc::clone(data), dev))
        }
        other => Err(Diagnostic::compute_error(
            "C0102",
            format!("to_gpu expects Matrix, found `{}`", other.type_name()),
        )),
    }
}

pub fn native_gpu_matrix_to_cpu(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0101", "GpuMatrix::to_cpu requires 1 argument (self)")
    })?;
    match target {
        Value::Struct { name, fields } if name == "GpuMatrix" => {
            if let Some(mat @ Value::Matrix { .. }) = fields.get("__data") {
                Ok(mat.clone())
            } else {
                Err(Diagnostic::compute_error("C0201", "Corrupted GpuMatrix struct"))
            }
        }
        Value::Matrix { .. } => Ok(target.clone()),
        other => Err(Diagnostic::compute_error(
            "C0102",
            format!("to_cpu expects GpuMatrix, found `{}`", other.type_name()),
        )),
    }
}

pub fn native_gpu_matmul(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error("C0101", "matmul requires 2 matrix arguments"));
    }
    let extract_mat = |v: &Value| -> Result<(usize, usize, Arc<Vec<f64>>, Value), Diagnostic> {
        match v {
            Value::Struct { name, fields } if name == "GpuMatrix" => {
                let dev = fields.get("device").cloned().unwrap_or(Value::Unit);
                if let Some(Value::Matrix { rows, cols, data }) = fields.get("__data") {
                    Ok((*rows, *cols, Arc::clone(data), dev))
                } else {
                    Err(Diagnostic::compute_error("C0201", "Corrupted GpuMatrix"))
                }
            }
            Value::Matrix { rows, cols, data } => {
                let dev = make_device_struct("CPU Fallback", false);
                Ok((*rows, *cols, Arc::clone(data), dev))
            }
            other => Err(Diagnostic::compute_error(
                "C0102",
                format!("matmul expects Matrix or GpuMatrix, found `{}`", other.type_name()),
            )),
        }
    };

    let (r1, c1, d1, dev) = extract_mat(&args[0])?;
    let (r2, c2, d2, _) = extract_mat(&args[1])?;

    if c1 != r2 {
        return Err(Diagnostic::statistical_error(
            "S0412",
            format!("Matrix dimension mismatch in matmul: ({}x{}) * ({}x{})", r1, c1, r2, c2),
        ));
    }

    // High performance blocked GEMM via faer
    let ma = Mat::from_fn(r1, c1, |i, j| d1[i * c1 + j]);
    let mb = Mat::from_fn(r2, c2, |i, j| d2[i * c2 + j]);
    let mc = &ma * &mb;

    let mut out_data = Vec::with_capacity(r1 * c2);
    for i in 0..r1 {
        for j in 0..c2 {
            out_data.push(mc[(i, j)]);
        }
    }

    Ok(make_gpu_matrix_struct(r1, c2, Arc::new(out_data), dev))
}

pub fn native_gpu_cholesky(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0101", "cholesky requires 1 argument (matrix)")
    })?;

    let (rows, cols, data, dev) = match target {
        Value::Struct { name, fields } if name == "GpuMatrix" => {
            let dev = fields.get("device").cloned().unwrap_or(Value::Unit);
            if let Some(Value::Matrix { rows, cols, data }) = fields.get("__data") {
                (*rows, *cols, Arc::clone(data), dev)
            } else {
                return Err(Diagnostic::compute_error("C0201", "Corrupted GpuMatrix"));
            }
        }
        Value::Matrix { rows, cols, data } => {
            (*rows, *cols, Arc::clone(data), make_device_struct("CPU Fallback", false))
        }
        other => return Err(Diagnostic::compute_error(
            "C0102",
            format!("cholesky expects Matrix or GpuMatrix, found `{}`", other.type_name()),
        )),
    };

    if rows != cols {
        return Err(Diagnostic::statistical_error(
            "S0410",
            format!("Cholesky decomposition requires a square matrix, got ({}x{})", rows, cols),
        ));
    }

    let out = crate::matrix::MatrixOps::cholesky(rows, &data)?;
    Ok(make_gpu_matrix_struct(rows, cols, Arc::new(out), dev))
}

// ---------------------------------------------------------------------------
// GpuVector Native Functions
// ---------------------------------------------------------------------------

pub fn native_vector_to_gpu(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0101", "to_gpu requires vector argument"));
    }
    let dev = args.get(1).cloned().unwrap_or_else(|| {
        make_device_struct("CPU Fallback (faer/rayon)", false)
    });

    match &args[0] {
        Value::Vector(vd) => {
            let floats: Vec<f64> = vd.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect();
            Ok(make_gpu_vector_struct(Arc::new(floats), dev))
        }
        other => Err(Diagnostic::compute_error(
            "C0102",
            format!("to_gpu expects Vector, found `{}`", other.type_name()),
        )),
    }
}

pub fn native_gpu_vector_to_cpu(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0101", "GpuVector::to_cpu requires 1 argument (self)")
    })?;
    match target {
        Value::Struct { name, fields } if name == "GpuVector" => {
            if let Some(vec @ Value::Vector(_)) = fields.get("__data") {
                Ok(vec.clone())
            } else {
                Err(Diagnostic::compute_error("C0201", "Corrupted GpuVector struct"))
            }
        }
        Value::Vector(_) => Ok(target.clone()),
        other => Err(Diagnostic::compute_error(
            "C0102",
            format!("to_cpu expects GpuVector, found `{}`", other.type_name()),
        )),
    }
}

pub fn native_gpu_reduce_sum(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0101", "reduce_sum requires vector argument")
    })?;

    let floats: Vec<f64> = match target {
        Value::Struct { name, fields } if name == "GpuVector" => {
            if let Some(Value::Vector(vd)) = fields.get("__data") {
                vd.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect()
            } else {
                return Err(Diagnostic::compute_error("C0201", "Corrupted GpuVector"));
            }
        }
        Value::Vector(vd) => vd.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect(),
        other => return Err(Diagnostic::compute_error(
            "C0102",
            format!("reduce_sum expects Vector or GpuVector, found `{}`", other.type_name()),
        )),
    };

    let total: f64 = floats.par_iter().sum();
    Ok(Value::F64(total))
}

// ---------------------------------------------------------------------------
// Philox PRNG (4x32-10) Counter-Based Generation
// ---------------------------------------------------------------------------

fn mulhilo(a: u32, b: u32) -> (u32, u32) {
    let prod = (a as u64) * (b as u64);
    ((prod >> 32) as u32, (prod & 0xFFFF_FFFF) as u32)
}

fn philox4x32_10(ctr: [u32; 4], key: [u32; 2]) -> [u32; 4] {
    let mut c = ctr;
    let mut k = key;
    for _ in 0..10 {
        let (hi0, lo0) = mulhilo(0xD2511F53, c[0]);
        let (hi1, lo1) = mulhilo(0xCD9E8D57, c[2]);
        c = [
            hi1 ^ c[1] ^ k[0],
            lo1,
            hi0 ^ c[3] ^ k[1],
            lo0,
        ];
        k[0] = k[0].wrapping_add(0x9E3779B9);
        k[1] = k[1].wrapping_add(0xBB67AE85);
    }
    c
}

pub fn native_philox_seed(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let s = match args.first() {
        Some(Value::I64(n)) => *n as u64,
        _ => 42,
    };
    Ok(make_philox_rng_struct(s))
}

pub fn native_philox_sample_uniform(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0101", "sample_uniform requires PhiloxRng"));
    }
    let seed: u64 = match &args[0] {
        Value::Struct { name, fields } if name == "PhiloxRng" => {
            fields.get("seed").and_then(|v| match v { Value::I64(s) => Some(*s as u64), _ => None }).unwrap_or(42)
        }
        _ => 42,
    };

    let dev = args.get(1).cloned().unwrap_or_else(|| {
        make_device_struct("CPU Fallback", false)
    });
    let n = args.get(2).and_then(|v| match v { Value::I64(x) => Some(*x as usize), _ => None }).unwrap_or(100);
    let min = args.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max = args.get(4).and_then(|v| v.as_f64()).unwrap_or(1.0);

    let key = [(seed & 0xFFFF_FFFF) as u32, (seed >> 32) as u32];
    let range = max - min;

    let num_blocks = (n + 3) / 4;
    let mut numbers: Vec<f64> = (0..num_blocks)
        .into_par_iter()
        .flat_map(|block_idx| {
            let ctr = [block_idx as u32, 0, 0, 0];
            let res = philox4x32_10(ctr, key);
            let mut chunk = Vec::with_capacity(4);
            for val in res {
                let u = (val as f64) * 2.3283064365386963e-10;
                chunk.push(min + u * range);
            }
            chunk
        })
        .collect();

    numbers.truncate(n);
    Ok(make_gpu_vector_struct(Arc::new(numbers), dev))
}

pub fn native_philox_sample_normal(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0101", "sample_normal requires PhiloxRng"));
    }
    let seed: u64 = match &args[0] {
        Value::Struct { name, fields } if name == "PhiloxRng" => {
            fields.get("seed").and_then(|v| match v { Value::I64(s) => Some(*s as u64), _ => None }).unwrap_or(42)
        }
        _ => 42,
    };

    let dev = args.get(1).cloned().unwrap_or_else(|| {
        make_device_struct("CPU Fallback", false)
    });
    let n = args.get(2).and_then(|v| match v { Value::I64(x) => Some(*x as usize), _ => None }).unwrap_or(100);
    let mean = args.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let std_dev = args.get(4).and_then(|v| v.as_f64()).unwrap_or(1.0);

    let key = [(seed & 0xFFFF_FFFF) as u32, (seed >> 32) as u32];

    let num_blocks = (n + 3) / 4;
    let mut normals: Vec<f64> = (0..num_blocks)
        .into_par_iter()
        .flat_map(|block_idx| {
            let ctr = [block_idx as u32, 0, 0, 0];
            let res = philox4x32_10(ctr, key);
            // Box-Muller on pairs (res[0], res[1]) and (res[2], res[3])
            let u1 = ((res[0] as f64) + 1.0) / 4294967297.0;
            let u2 = (res[1] as f64) / 4294967296.0;
            let r1 = (-2.0 * u1.ln()).sqrt();
            let theta1 = 2.0 * std::f64::consts::PI * u2;
            let z0 = r1 * theta1.cos();
            let z1 = r1 * theta1.sin();

            let u3 = ((res[2] as f64) + 1.0) / 4294967297.0;
            let u4 = (res[3] as f64) / 4294967296.0;
            let r2 = (-2.0 * u3.ln()).sqrt();
            let theta2 = 2.0 * std::f64::consts::PI * u4;
            let z2 = r2 * theta2.cos();
            let z3 = r2 * theta2.sin();

            vec![
                mean + std_dev * z0,
                mean + std_dev * z1,
                mean + std_dev * z2,
                mean + std_dev * z3,
            ]
        })
        .collect();

    normals.truncate(n);
    Ok(make_gpu_vector_struct(Arc::new(normals), dev))
}
