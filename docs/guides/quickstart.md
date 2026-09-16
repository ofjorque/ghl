# Tutorial Rápido: GHL en 15 Minutos

¡Te damos la bienvenida a **GHL** (*Generalized Hypothesis Language*)! Este tutorial está diseñado para llevarte desde la descarga e instalación hasta tu primer análisis estadístico completo en menos de 15 minutos.

---

## 1. Instalación y Configuración del Entorno

GHL se distribuye como un binario nativo autónomo (`ghl`) sin dependencias externas obligatorias ni tiempos de espera de compilación.

### En Windows (PowerShell):
```powershell
# Ejecutar el script de instalación automática
.\install.ps1
```

### En Linux y macOS (Bash / Zsh):
```bash
# Ejecutar el instalador para Unix
./install.sh
```

Para verificar que la instalación se completó correctamente, ejecuta:
```bash
ghl version
```
Verás la tarjeta informativa con el motor JIT Cranelift, el runtime NEKO y las capacidades gráficas detectadas en tu terminal.

---

## 2. El Shell Interactivo (REPL)

GHL incluye un REPL de alto rendimiento (`ghl repl`) que inicia en menos de **20 milisegundos**. Para entrar:

```bash
ghl repl
```

Verás el saludo cálido y empático de bienvenida:
```text
┌───────────────────────── GHL Interactive Shell (REPL) ─────────────────────────┐
│ READY                                                                          │
│ Gojo & Haru High-Performance Statistical System                                │
│ Type :help for session commands, ?<fn> for docs, or :quit to exit.             │
└────────────────────────────────────────────────────────────────────────────────┘

ghl(=^･ω･^=)> 
```

### Comandos de Sesión Útiles en el REPL:
- `:help` o `:h`: Muestra la lista de comandos disponibles.
- `:vars` o `:v`: Lista las variables activas en tu sesión actual con sus tipos inferidos.
- `?<nombre_funcion>`: Consulta al instante la documentación matemática, parámetros y ejemplos de cualquier función de la biblioteca estándar (ej. `?mean`, `?ols`, `?fit_logistic`, `?cholesky`).
- `?`: Lista todas las funciones documentadas en la biblioteca estándar.
- `:clear` o `:c`: Limpia la pantalla del terminal.
- `:quit` o `:q`: Cierra la sesión interactiva.

#### Ejemplo Rápido en el REPL:
```ghl
ghl(=^･ω･^=)> let x = [10.0, 25.0, 32.0, 48.0, 50.0];
ghl(=^･ω･^=)> mean(x)
33.0
ghl(=^･ω･^=)> std_dev(x)
16.294170736798784
ghl(=^･ω･^=)> ?mean
```

---

## 3. "Hola Mundo" Estadístico

Vamos a construir un script completo en GHL que cargue datos, realice limpieza y transformación columnar, ajuste un modelo de regresión lineal OLS con fórmulas de Wilkinson-Rogers, y exporte un gráfico visual.

Crea un archivo llamado `hello_stats.gh` con tu editor preferido (o Positron / VS Code con la extensión GHL):

```ghl
// hello_stats.gh
// A complete statistical workflow in GHL

// 1. Define synthetic experiment dataset
let data = dataframe {
    patient_id: [1, 2, 3, 4, 5, 6, 7, 8],
    treatment: ["A", "A", "A", "A", "B", "B", "B", "B"],
    dosage: [10.0, 20.0, 30.0, 40.0, 10.0, 20.0, 30.0, 40.0],
    biomarker: [15.2, 28.4, 39.1, 51.0, 22.1, 36.8, 52.4, 67.2]
};

// 2. Data processing and exploration pipeline
let high_dosage = data
    |> filter(dosage >= 20.0)
    |> mutate(response_per_dose = biomarker / dosage)
    |> arrange(desc(response_per_dose));

// 3. Statistical modeling: fit Ordinary Least Squares (OLS)
// Response variable: biomarker, Predictors: dosage and treatment
let model = ols(biomarker ~ dosage + treatment, data);

// 4. Inspect model inference with Cockpit Deck
summary(model);

// 5. Generate predictions and evaluate residuals
let preds = predict(model, data);
let res = residuals(model);

// 6. Generate and export diagnostic scatter plot
let p = plot(data, aes(x = dosage, y = biomarker, color = treatment))
    |> geom_point()
    |> geom_line();

show_plot(p, "scatter_treatment.svg");
```

### Ejecución del Script:

Ejecuta el archivo directamente con el compilador JIT de GHL:
```bash
ghl run hello_stats.gh
```

Observarás en tu consola la tabla de diagnósticos estadísticos **Cockpit Deck** con los coeficientes estimados ($\hat{\beta}$), errores estándar, estadísticos $t$, valores $p$, $R^2$, AIC y BIC, junto con el archivo gráfico vectorial `scatter_treatment.svg` generado en disco.

---

## 4. Estructura de Proyectos con `ghl new`

Para proyectos estadísticos más grandes y reproducibles, GHL incluye un gestor de paquetes y proyectos integrado (RFC 06):

```bash
# Crear un nuevo proyecto estructurado
ghl new clinical_trial

# Entrar al directorio
cd clinical_trial

# Estructura generada:
# clinical_trial/
# ├── ghl.toml     (Metadatos y dependencias del proyecto)
# └── src/
#     └── main.gh  (Punto de entrada principal)
```

Para validar tipos y sintaxis sin ejecutar:
```bash
ghl check src/main.gh
```

Para dar formato canónico a todo el código:
```bash
ghl fmt src/main.gh
```

Para correr las pruebas unitarias y estadísticas:
```bash
ghl test
```

¡Listo! Ya conoces lo esencial para comenzar a explorar y modelar datos con **GHL**. Continúa con la guía de recetas en [`cookbook.md`](cookbook.md) para técnicas estadísticas avanzadas.
