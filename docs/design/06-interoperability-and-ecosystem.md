# RFC 06: Interoperabilidad y Ecosistema de GHL

- **Estado:** Propuesto
- **Área:** Ecosistema / FFI / Gestión de Paquetes
- **Audiencia:** Desarrolladores de paquetes, integradores de sistemas

---

## 1. Interoperabilidad Nativa Zero-Copy

GHL se integra directamente con el ecosistema analítico moderno a través de la especificación **Apache Arrow C Data Interface**:

```
                  ┌──────────────────────────────┐
                  │    Apache Arrow Buffer       │
                  │ (Zero-Copy Shared Memory)    │
                  └──────────────┬───────────────┘
                                 │
         ┌───────────────────────┼───────────────────────┐
         ▼                       ▼                       ▼
      [ GHL ]           [ Python / PyArrow ]         [ R / Arrow ]
  High-speed native      Zero-copy ingestion       Zero-copy plotting
```

---

## 2. Enlace Nativo C-ABI

Llamar a bibliotecas en C o Fortran (como BLAS, LAPACK o bibliotecas del sistema operativo) se realiza sin overhead:

```lang
extern "C" {
    fn dgemm_(
        transa: *const u8, transb: *const u8,
        m: *const i32, n: *const i32, k: *const i32,
        alpha: *const f64, a: *const f64, lda: *const i32,
        b: *const f64, ldb: *const i32,
        beta: *const f64, c: *mut f64, ldc: *mut i32
    );
}
```

---

## 3. Puentes con Python y R

### Consumo Fluido de Librerías Externas
```lang
use std::interop::python;

let py_math = python::import("scipy.spatial.distance")?;
let py_matrix = python::to_numpy(&my_ghl_matrix);
let distance_matrix = py_math.pdist(py_matrix);
```

### Exportación hacia R y Python
El compilador GHL genera extensiones compartidas nativas (`.so` / `.dylib`) con anotaciones simples:
```lang
#[export_ffi]
pub fn custom_mcmc_sampler(data: &[f64], iterations: usize) -> Vec<f64> {
    // Pure compiled GHL code callable directly from R or Python
}
```

---

## 4. Gestor de Paquetes y CLI de GHL (`ghl`)

La herramienta de línea de comandos `ghl` provee un flujo hermético y reproducible:
- `ghl new <project_name>`: Crea un proyecto con plantilla estructurada.
- `ghl fetch`: Recupera y verifica todas las dependencias declaradas en `ghl.toml`.
- `ghl test`: Ejecuta la suite de pruebas unitarias y estadísticas.
- `ghl build --release`: Compila a un binario nativo independiente con optimizaciones de LLVM.
- `ghl.lock`: Archivo con hashes SHA-256 de todas las dependencias para garantizar reproducibilidad exacta en cualquier máquina.

