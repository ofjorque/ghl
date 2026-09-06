# RFC 10: Presupuestos de Rendimiento, Tamaño y Garantías de Ergonomía (SLAs)

- **Estado:** Aprobado
- **Área:** Calidad de Ingeniería / SLAs / Presupuesto de Rendimiento / Métricas
- **Audiencia:** Desarrolladores del compilador, colaboradores de IA, evaluadores de QA

---

## 1. El Rol de los Presupuestos de Rendimiento (Performance Budgets)

Un error frecuente en proyectos de lenguajes es tratar el rendimiento y el tamaño como objetivos abstractos ("debe ser rápido") en lugar de **contratos medibles**. Sin límites concretos, el software acumula regresiones invisibles: el tiempo de arranque pasa de 15 ms a 300 ms, los binarios crecen de 10 MB a 200 MB, y las funciones estadísticas comienzan a duplicar memoria en silencio.

Este documento establece los **Presupuestos Innegociables de GHL**. Ningún commit, refactorización o propuesta de optimización será aceptada si vulnera estos umbrales cuantitativos.

---

## 2. Presupuestos de Velocidad y Latencia (Speed Budgets)

```
+--------------------------------------------------------------------------------+
| METAS DE VELOCIDAD Y TIEMPOS DE RESPUESTA (BENCHMARKS OFICIALES)               |
+------------------------------------+-----------------------+-------------------+
| Dimensión / Tarea                  | Límite Máximo Aceptable| Meta de Referencia |
+------------------------------------+-----------------------+-------------------+
| Arranque en frío (Cold CLI Start)  | ≤ 15 ms               | ≤ 8 ms            |
| Latencia por línea en el REPL      | ≤ 10 ms               | ≤ 3 ms            |
| Compilación JIT de función simple  | ≤ 25 ms               | ≤ 12 ms           |
| Bucles escalares puros (vs C/Rust) | Factor de 0.95x a 1.05x | Paridad nativa (1.0x) |
| Álgebra Lineal (vs OpenBLAS/MKL)   | ≥ 90% de velocidad    | ≥ 98% de velocidad|
| Ingestión de CSV / Parquet         | ≥ 250 MB/s por núcleo | ≥ 500 MB/s por núcleo |
+------------------------------------+-----------------------+-------------------+
```

### 2.1. Cero Tolerancia con la Latencia TTFX
- A diferencia de Julia (donde el usuario espera segundos para compilar una función interactiva), GHL garantiza que cualquier script estándar de análisis exploratorio muestre su primer resultado en **menos de 20 milisegundos**.
- Si una modificación en el frontend (`logos` o `chumsky`) incrementa el tiempo de parseo por encima de 10 ms para archivos de hasta 10.000 líneas, el cambio se considerará defectuoso.

---

## 3. Presupuestos de Tamaño y Memoria (Size & Memory Budgets)

```
+--------------------------------------------------------------------------------+
| METAS DE HUELLA DE MEMORIA Y TAMAÑO DE ARTEFACTOS                              |
+------------------------------------+-----------------------+-------------------+
| Dimensión                          | Límite Máximo Aceptable| Meta de Referencia |
+------------------------------------+-----------------------+-------------------+
| Tamaño de binario AOT (`--release`)| ≤ 20 MB               | ≤ 12 MB           |
| Memoria en reposo (Idle RSS)       | ≤ 10 MB               | ≤ 6 MB            |
| Asignaciones en Heap por iteración | Estrictamente 0       | 0 (Arena Bump)    |
| Clonado de DataFrame / Matriz      | O(1) tiempo / memoria | O(1) vía CoW      |
| Sobrecarga de memoria para `NA`    | 1 bit por fila        | 1 bit (Arrow mask)|
+------------------------------------+-----------------------+-------------------+
```

### 3.1. Invariante de Heap en Bucles de Simulación (MCMC & Bootstrap)
- En bucles iterativos de muestreo o remuestreo:
  ```lang
  for step in 0..1_000_000 {
      let sample = simulate_step(&mut state);
  }
  ```
  El conteo de asignaciones en el Heap de propósito general del sistema operativo debe ser **exactamente CERO**. Todos los vectores y matrices intermedias deben alojarse en la **Arena Regional** (`bumpalo`) y reciclarse con coste $O(1)$.

### 3.2. Tamaño del Binario Autocontenido
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

