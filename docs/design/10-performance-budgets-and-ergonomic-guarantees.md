# RFC 10: Presupuestos de Rendimiento, Tamaño y Garantías de Ergonomía (SLAs)

- **Estado:** Aprobado (revisión 2 — calibrado con benchmarks Linux)
- **Área:** Calidad de Ingeniería / SLAs / Presupuesto de Rendimiento / Métricas
- **Audiencia:** Desarrolladores del compilador, colaboradores de IA, evaluadores de QA

---

## 1. El Rol de los Presupuestos de Rendimiento (Performance Budgets)

Un error frecuente en proyectos de lenguajes es tratar el rendimiento y el tamaño como objetivos abstractos ("debe ser rápido") en lugar de **contratos medibles**. Sin límites concretos, el software acumula regresiones invisibles: el tiempo de arranque pasa de 15 ms a 300 ms, los binarios crecen de 10 MB a 200 MB, y las funciones estadísticas comienzan a duplicar memoria en silencio.

Este documento establece los **Presupuestos Innegociables de GHL**. Ningún commit, refactorización o propuesta de optimización será aceptada si vulnera estos umbrales cuantitativos.

---

## 2. Presupuestos de Velocidad y Latencia (Speed Budgets)

```
+----------------------------------------------------------------------------------------------+
| METAS DE VELOCIDAD Y TIEMPOS DE RESPUESTA (BENCHMARKS OFICIALES — Linux x86-64, 30 runs)    |
+----------------------------------------------+--------------------+------------------------+
| Dimensión / Tarea                            | Límite Máximo      | Meta de Referencia     |
+----------------------------------------------+--------------------+------------------------+
| Arranque en frío (Cold CLI Start — AOT)      | ≤ 15 ms            | ≤ 1 ms  (actual: 0.9 ms)|
| Latencia por línea en el REPL                | ≤ 10 ms            | ≤ 3 ms                 |
| Bucles escalares puros (vs NumPy)            | ≥ 2.0x más rápido  | ≥ 2.5x (actual: 18.1x) |
| Álgebra Lineal / Dot product (vs NumPy)      | ≥ 1.5x más rápido  | ≥ 2.0x (actual: 2.28x) |
| Ingestión CSV texto delimitado (multihilo)   | ≥ 300 MB/s total   | ≥ 400 MB/s total       |
| Ingestión CSV texto delimitado (por núcleo)  | ≥ 60 MB/s/núcleo   | ≥ 100 MB/s/núcleo      |
| Ingestión Parquet columnar binario (total)   | ≥ 250 MB/s total   | ≥ 500 MB/s total       |
| DataFrame 1M filas vs Python Polars          | ≤ 1.30x más lento  | Paridad (1.0x)         |
+----------------------------------------------+--------------------+------------------------+
```

> **Nota de calibración (revisión 2):** El presupuesto original de §2 unificaba CSV y Parquet bajo
> "≥ 250 MB/s por núcleo". Los benchmarks Linux reproducibles (Suite 02, 30 runs) muestran que el
> parseo de texto CSV alcanza ~35–50 MB/s por núcleo (~300 MB/s multihilo) — cumple el objetivo
> multihilo pero no el por-núcleo. Parquet columnar binario supera 250 MB/s total. El presupuesto
> ahora diferencia explícitamente ambos formatos.

### 2.1. Cero Tolerancia con la Latencia TTFX
- A diferencia de Julia (donde el usuario espera segundos para compilar una función interactiva), GHL garantiza que cualquier script estándar de análisis exploratorio muestre su primer resultado en **menos de 20 milisegundos**.
- Si una modificación en el frontend (`logos` o `chumsky`) incrementa el tiempo de parseo por encima de 10 ms para archivos de hasta 10.000 líneas, el cambio se considerará defectuoso.

---

## 3. Presupuestos de Tamaño y Memoria (Size & Memory Budgets)

```
+----------------------------------------------------------------------------------------------+
| METAS DE HUELLA DE MEMORIA Y TAMAÑO DE ARTEFACTOS                                           |
+---------------------------------------------+--------------------+------------------------+
| Dimensión                                   | Límite Máximo      | Meta de Referencia     |
+---------------------------------------------+--------------------+------------------------+
| Tamaño de binario AOT (`--release`)         | ≤ 20 MB            | ≤ 12 MB                |
| Memoria en reposo (Idle RSS)                | ≤ 10 MB            | ≤ 6 MB                 |
| Asignaciones en Heap por iteración (for/while)| Estrictamente 0  | 0 (scope reutilizado)  |
| Clonado de DataFrame                        | O(1) tiempo/mem    | O(1) vía Arc<Column>   |
| Clonado de Matriz (`Value::Matrix`)         | O(1) tiempo/mem    | O(1) vía Arc<Vec<f64>> |
| NA estándar (Arrow validity mask)           | 1 bit por fila     | 1 bit (Arrow mask)     |
| NA:Reason (side-channel semántico)          | O(motivos únicos)  | Arc<str> ptr por copia |
+---------------------------------------------+--------------------+------------------------+
```

### 3.1. Invariante de Heap en Bucles de Simulación (MCMC & Bootstrap)
- En bucles iterativos de muestreo o remuestreo, el conteo de asignaciones en el Heap debe ser **exactamente CERO** por iteración:
  ```lang
  for step in 0..1_000_000 {
      let sample = simulate_step(state);
      state = sample;
  }
  ```
- **Estado:** `for x in a..b { body }` está implementado (Fase C, sept 2026). El compilador reutiliza el slot de la variable de bucle en cada iteración sin asignación extra.
- **Modelo de mutabilidad:** GHL no expone referencias `&mut`. El estado se actualiza vía reasignación CoW: `state = simulate_step(state)`. Cuando `Arc::strong_count == 1`, el update es in-place ($O(1)$) sin copiar datos.
- Todos los vectores y matrices intermedias deben alojarse en la **Arena Regional** (`bumpalo`) y reciclarse con coste $O(1)$ (TODO.md Fase 5+).

### 3.2. Clonado CoW de Matrices y Motivos NA (Fase A & B — implementado)
- `Value::Matrix` almacena `data: Arc<Vec<f64>>`. Clonar la variante es $O(1)$ y no duplica el búfer.
- `NaReasonTable` almacena razones como `Arc<str>`. Operaciones de reindexado (`reindex`, `rename_column`, `retain_columns`, `shift`) clonan el puntero Arc, no la cadena de texto.

### 3.3. Tamaño del Binario Autocontenido
- Los binarios generados con `ghl build --release` no deben superar los **15 a 20 MB** (después de aplicar `strip` y LTO).
- **Prohibido:** No se permite empaquetar compiladores pesados de C/C++ ni dependencias de runtime de cientos de megabytes en los binarios ejecutables de usuario.

---

## 4. Garantías de Ergonomía y Empatía de UX (UX SLAs)

1. **Cero Pánicos Crípticos de Rust:**
   - Ningún error sintáctico, de tipos o excepción de usuario en GHL debe exponer jamás un `panic!` interno de Rust con tracebacks del compilador.
   - Todo fallo debe interceptarse y canalizarse a través de **`ariadne`** con formato Kaomoji.
2. **Diagnósticos en Menos de 3 Segundos de Comprensión:**
   - Todo mensaje de error debe indicar:
     1. Archivo, línea y columna con visualización del código.
     2. Subrayado del token infractor.
     3. Código formal (`[Compute Error Cxxxx]` o `[Statistical Error Sxxxx]`).
     4. Explicación conceptual clara sin tecnicismos innecesarios.
     5. Sugerencia accionable (*"Help: Did you mean...?"* o *"Remedy: Regularize covariance..."*).
3. **Reproducibilidad Estricta de Semillas (Bit-for-Bit PRNG):**
   - Una semilla dada debe generar exactamente la misma secuencia de flotantes `f64` en procesadores Intel x86_64, AMD x86_64 y Apple Silicon ARM64.

