# RFC 00: Visión, Filosofía y Principios Rectores de GHL

- **Estado:** Propuesto
- **Área:** Arquitectura General / Fundamentos
- **Audiencia:** Diseñadores del lenguaje, implementadores del compilador, usuarios

---

## 1. El Problema que Resuelve GHL

El desarrollo de software estadístico y científico contemporáneo vive fragmentado en dos mundos disjuntos:

1. **Lenguajes de scripting dinámico (R, Python):**
   - **Ventajas:** Sintaxis expresiva para análisis exploratorio, ecosistema masivo, manipulación intuitiva de datos.
   - **Desventajas:** El "problema de los dos lenguajes". Escribir bucles nativos en Python o R degrada el rendimiento en órdenes de magnitud (\~50x - 200x más lento). Para lograr velocidad, las librerías críticas (`numpy`, `pandas`, `torch`, `data.table`) están escritas en C, C++, Cython o Rust. Esto genera barreras insalvables para el usuario común cuando necesita implementar un nuevo estimador, un algoritmo Monte Carlo o una función de pérdida personalizada sin recurrir a lenguajes de bajo nivel.

2. **Lenguajes JIT científicos (Julia):**
   - **Ventajas:** Resuelve los dos lenguajes permitiendo escribir código rápido directamente en Julia; múltiple despacho elegante.
   - **Desventajas:** 
     - *Time to First Plot (TTFX):* La compilación en tiempo de ejecución (JIT) introduce latencias notables que perjudican la experiencia de línea de comandos, scripting y microservicios.
     - *Recolector de Basura (GC):* Las pausas no deterministas del GC complican la ejecución de simulaciones críticas de baja latencia o la integración fluida como biblioteca compartida en otros entornos.
     - *No generalista en la práctica:* Dificultad para generar binarios pequeños y autónomos para producción, sistemas embebidos o utilitarios CLI.

3. **Lenguajes de sistemas modernos (Rust, Go, Swift):**
   - **Ventajas:** Rendimiento predecible, seguridad de memoria, binarios autocontenidos, ecosistemas de ingeniería sólidos.
   - **Desventajas:** Sintaxis verbosa para álgebra matricial y operaciones estadísticas; falta de conceptos nativos de datos ausentes (`NA`) con lógica ternaria; ausencia de operadores de fórmula o dataframes ergonómicos como tipos primitivos.

---

## 2. La Visión de GHL (*Generalized Hypothesis Language*)

Crear un **lenguaje de programación generalista** con la velocidad, portabilidad y previsibilidad de **Rust**, pero con la expresividad matemática y ergonomía estadística de **R y Julia**, envuelto en una experiencia de usuario empática y cálida.

**Identidad del proyecto:** Nombrado públicamente como **GHL** (*Generalized Hypothesis Language*) y concebido internamente en tributo a los gatos **Gojo & Haru**, GHL busca que la computación matemática rigurosa no se sienta hostil ni esotérica. El lenguaje adopta una personalidad accesible, amigable con las mascotas y los investigadores, integrando Kaomojis en sus diagnósticos sin comprometer un ápice de rigor formal.

No es un DSL (Domain Specific Language): es un lenguaje en el que se puede escribir un servidor HTTP asíncrono, un motor de base de datos columnar, un script de shell, un paquete de simulación MCMC o un pipeline de aprendizaje automático con la misma herramienta y sin cambios de paradigma.

---

## 3. Principios Rectores (Invariantes de Diseño)

### I. Cero Compromiso en Rendimiento y Determinismo
- Todo el código se compila a código nativo (Ahead-Of-Time / AOT) utilizando un backend moderno (LLVM o Cranelift).
- Sin pausas imprevistas de recolección de basura: la gestión de memoria se basa en ARC optimizado con semántica CoW (Copy-on-Write) y arenas para cómputo masivo temporal.

### II. Primera Clase para Conceptos Estadísticos
- Los vectores, tensores, matrices y marcos de datos son tipos nativos y reconocidos por el compilador, no adaptadores externos.
- Manejo formal de valores ausentes (`NA`) respetando la lógica trivalente de Kleene, garantizando que el tratamiento de datos incompletos sea consistente en todo el lenguaje.

### III. Taxonomía Dual de Diagnósticos con Empatía
- El compilador distingue claramente entre **fallos del sistema informático** (`[Compute Error]`) y **anomalías en el modelo matemático/estadístico** (`[Statistical Error]`).
- Diagnósticos empáticos enriquecidos con Kaomojis para orientar al usuario en lugar de frustrarlo.

### IV. Reproducibilidad Matemática Garantizada
- Soporte para generadores de números pseudoaleatorios (PRNG) reproducibles a nivel de bit entre plataformas y arquitecturas (ej. x86_64 vs ARM64).
- Control estricto sobre optimizaciones de punto flotante: modo IEEE-754 estricto por defecto, con flags explícitos para optimizaciones *fast-math* cuando el usuario lo autorice.

### V. Interoperabilidad Nativa Zero-Copy
- Compatibilidad directa con el estándar de memoria en columnas **Apache Arrow**.
- FFI nativo C-ABI sin coste adicional, y conectores limpios para interactuar con librerías de Python (NumPy/PyTorch) y R sin serializaciones redundantes.

