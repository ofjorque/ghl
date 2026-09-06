# Suite 04: Características del Runtime, Latencia y Huella del Sistema en GHL

- **Área:** Rendimiento del Runtime / Tiempos de Arranque / Huella de Recursos
- **Objetivo:** Evaluar la viabilidad práctica del lenguaje para utilitarios de línea de comandos, microservicios, producción en la nube y entornos de recursos limitados.

---

## 1. Casos de Prueba Especificados

### Caso 4.1: Latencia de Arranque en Frío y TTFX (Time to First Execution)
Medición con la herramienta `hyperfine` desde la terminal del sistema operativo sin demonios previos en memoria:
- **Prueba A (Script Mínimo):** Ejecución de `hello_world.gh` en frío.
- **Prueba B (Script de Álgebra Lineal):** Cargar biblioteca de álgebra lineal, crear matriz $10 \times 10$, resolver sistema $A \cdot x = b$ e imprimir solución.
- **Prueba C (Script de Datos):** Cargar tabla de 100.000 filas, filtrar e imprimir la media de una columna.

#### Metas de Rendimiento Esperadas:
| Entorno / Lenguaje | Tiempo Prueba A (Mínimo) | Tiempo Prueba B (Álgebra) | Tiempo Prueba C (Datos) |
| :--- | :--- | :--- | :--- |
| **Python** | ~35 ms | ~120 ms | ~250 ms |
| **R** | ~50 ms | ~70 ms | ~180 ms |
| **Julia** | ~250 ms | **~1.500 ms - 3.000 ms** (JIT) | **~3.000 ms - 6.000 ms** (JIT) |
| **GHL (AOT Nativo)** | **< 10 ms** | **< 15 ms** | **< 25 ms** |

---

## 2. Huella de Memoria Base (Idle Memory Footprint)

Medición del consumo de memoria física (RSS) con el entorno recién arrancado e inactivo:

- **Python (CPython):** ~12 MB de memoria base (se incrementa a > 40 MB al importar NumPy y Pandas).
- **R:** ~30 MB de memoria base (se incrementa a > 70 MB al importar tidyverse).
- **Julia:** **~180 MB - 300 MB** de memoria base tan pronto arranca el REPL y el runtime LLVM.
- **GHL:** **~3 MB - 8 MB** de memoria base para ejecutables AOT; ~25 MB en modo REPL interactivo.

---

## 3. Tamaño de Binarios y Despliegue Autónomo

Evaluación de la capacidad de distribuir una aplicación estadística completa a producción sin instalar dependencias globales en el servidor de destino:

| Enfoque de Distribución | Tamaño Típico del Artefacto | Dependencias Externas Requeridas |
| :--- | :--- | :--- |
| **Python (PyInstaller / Docker)** | 150 MB - 1 GB | Intérprete CPython + librerías C compartidas |
| **R (Rscript / Docker)** | 400 MB - 1.2 GB | Runtime R completo + paquetes CRAN |
| **Julia (PackageCompiler.jl)** | 350 MB - 800 MB | Runtime Julia + librerías LLVM |
| **GHL (`ghl build --release`)** | **8 MB - 20 MB** | **Ninguna (Binario ELF/Mach-O autónomo)** |
