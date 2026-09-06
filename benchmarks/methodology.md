# Metodología de Benchmarking y Criterios de Imparcialidad en GHL

- **Área:** Evaluación Experimental / Calidad de Rendimiento
- **Audiencia:** Desarrolladores de GHL, evaluadores de rendimiento, investigadores

---

## 1. Principios de Imparcialidad y Rigor

Para garantizar conclusiones justas y científicamente sólidas al comparar **GHL** contra R, Python y Julia:

1. **Código Idiomático y Óptimo en Cada Lenguaje:**
   - **R:** Evaluado usando tanto funciones base optimizadas como las herramientas estándar de la comunidad (`data.table`, `collapse`, BLAS multi-hilo acelerado).
   - **Python:** Evaluado usando `NumPy`, `Polars`/`Pandas`, `Numba` (para bucles compilados JIT) y `PyTorch`.
   - **Julia:** Evaluado asegurando estabilidad de tipos (*type-stability*), sin variables globales no tipadas y con anotaciones `@inbounds` donde corresponda.
   - **GHL:** Evaluado en compilación de producción AOT (`ghl build --release`) y en modo interactivo (`ghl run`).

2. **Idéntico Tratamiento de Datos:**
   - Todos los benchmarks consumen exactamente los mismos conjuntos de datos de entrada, pregenerados en formato estándar **Apache Arrow IPC / Feather** o mediante generadores pseudoaleatorios deterministas con semilla fija.

---

## 2. Dimensiones y Métricas de Medición

Para cada prueba se registran cuatro dimensiones:

### A. Tiempo de Ejecución en Régimen Permanente (Steady-State Wall-Clock Time)
- Medición del bucle caliente una vez inicializado el entorno.
- Se descartan las primeras ejecuciones de calentamiento (*warmup*).
- Se reporta la **mediana** y el **rango intercuartílico (IQR)** de al menos 30 iteraciones independientes.

### B. Tiempo hasta la Primera Ejecución (TTFX - Time to First Execution)
- Tiempo total medido desde la invocación del comando en shell en frío hasta la finalización de la tarea:
  $$\text{TTFX} = T_{\text{arranque del proceso}} + T_{\text{parseo/compilación/importación}} + T_{\text{primera ejecución}}$$
- Crítico para utilitarios de línea de comandos, scripts de análisis rápido y microservicios.

### C. Huella de Memoria Máxima (Peak RSS)
- Medición de la memoria física residente máxima consumida durante todo el ciclo de vida del proceso mediante `/usr/bin/time -v` (`Maximum resident set size`) y perfiladores de heap.

### D. Conteo y Pausas de Recolección de Basura
- Tiempo acumulado invertido en pausas de GC frente al tiempo neto de cálculo útil. En GHL esta métrica es siempre **0.00 ms** debido a la ausencia de Tracing GC.

---

## 3. Protocolo de Aislamiento de Hardware

Para minimizar el ruido térmico y la interferencia del sistema operativo:
1. **Gobernador de CPU:** Frecuencia fijada en `performance` (desactivar *powersave* y *ondemand*).
2. **Desactivación de Turbo Boost:** Para evitar fluctuaciones debidas a estrangulamiento térmico (*thermal throttling*).
3. **Afinidad de Procesador (`taskset`):** Los procesos monohilo se anclan a núcleos de CPU específicos y aislados (`isolcpus`).
4. **Control de Hilos:** En pruebas monohilo se fuerzan variables de entorno explícitas:
   ```bash
   export OMP_NUM_THREADS=1
   export OPENBLAS_NUM_THREADS=1
   export MKL_NUM_THREADS=1
   export JULIA_NUM_THREADS=1
   export GHL_NUM_THREADS=1
   ```
