# RFC 14 — Protocolos de Expectativas y Contratos Estadísticos (`spring_pact`)

> *"Los análisis estadísticos no deben depender de acuerdos verbales ni de decisiones metodológicas ad-hoc: un contrato ejecutable garantiza la transparencia, la reproducibilidad y el rigor entre las partes."*

- **Versión**: 0.1.0 (Propuesta)
- **Estado**: En Diseño
- **Área**: Modelado Estadístico / Auditoría / Primera Librería Nativa en GHL (`spring_pact`)
- **Tributo**: Nombrado `spring_pact` en honor a **Haru** (*Haru* = 春 = Primavera en japonés).

---

## 1. Motivación y Visión

En la práctica analítica, regulatoria y científica moderna (consultorías, ensayos clínicos, auditorías económicas, pre-registros de hipótesis), existe una brecha constante entre quien **solicita o audita** un análisis y quien lo **ejecuta**:
1. **Manipulación inadvertida de datos**: Filtrado arbitrario de casos extremos (*cherry-picking* de rangos de edad, eliminación injustificada de observaciones).
2. **Violación silenciada de supuestos**: Aplicación de regresiones OLS en presencia de heterocedasticidad severa sin corregir errores estándar, o con multicolinealidad extrema ($VIF > 10$).
3. **Falta de trazabilidad criptográfica**: Imposibilidad de verificar si el script analizado respetó el protocolo pactado originalmente o si las reglas cambiaron durante el análisis.

`spring_pact` resuelve este problema introduciendo **Contratos Estadísticos Ejecutables**:
- Un archivo de contrato declarativo define formalmente las expectativas sobre los datos y los modelos.
- El contrato genera un hash criptográfico **SHA-256** determinista e inmutable.
- Al ejecutar el análisis, GHL evalúa el contrato contra los datos y el modelo, emitiendo un **Certificado de Cumplimiento (Cockpit Deck Receipt)** con firmas digitales de los datos, el contrato y los estimadores resultantes.

---

## 2. Arquitectura de `spring_pact` (100% GHL Puro)

`spring_pact` se concibe como la **primera librería oficial desarrollada 100% en código fuente GHL**, aprovechando las capacidades del lenguaje:
- Gestión de paquetes estándar (`ghl new spring_pact`, manifiesto `ghl.toml` y `ghl.lock`).
- Operadores pipeline (`|>`), registros y estructuras de datos nativas.
- Lógica trivalente de valores faltantes (`NA:Reason`).
- Telemetría y renderizado visual en terminal con **Cockpit Deck** (`render_cockpit`).

```
                ┌──────────────────────────────────────────────┐
                │      spring_pact::Contract (SHA-256)         │
                └──────────────────────┬───────────────────────┘
                                       │
         ┌─────────────────────────────┼─────────────────────────────┐
         ▼                             ▼                             ▼
  Data Expectations            Model Assumptions             Auditing & Receipt
  • expect_filter()            • expect_vif_max()            • verify(model)
  • expect_na_tolerance()      • expect_homoscedasticity()   • issue_certificate()
  • expect_outlier_rule()      • expect_min_sample_size()    • Cockpit Deck Panel
```

---

## 3. Especificación del Contrato

### 3.1. Dimensiones de Expectativas

1. **Expectativas de Limpieza e Inclusión (Data Expectations)**:
   - `expect_range(col, min, max)`: Restricción de valores admisibles (ej. edad $\ge 18$).
   - `expect_na_max(col, threshold)`: Tolerancia máxima de valores faltantes (ej. $NAs \le 2\%$).
   - `expect_no_outliers(col, method, factor)`: Detección formal de valores extremos (vía $IQR$ o $Z\text{-score}$).

2. **Expectativas de Supuestos del Modelo (Model Assumptions)**:
   - `expect_vif(max_val)`: Factor de inflación de la varianza para evitar multicolinealidad.
   - `expect_residual_normality(p_value_min)`: Test de normalidad en residuos (Shapiro-Wilk / Jarque-Bera).
   - `expect_vcov_policy(allowed_kinds)`: Exigencia de errores estándar robustos si se detecta heterocedasticidad.
   - `expect_min_dof(min_df)`: Grados de libertad mínimos para validez asintótica.

3. **Expectativas de Causalidad y Robustez**:
   - `expect_coefficient_sign(term, expected_sign)`: Invarianza de dirección del efecto estimado.

---

## 4. Ejemplo de Código en GHL

```ghl
use std::dataframe::*;
use std::stats::*;
use spring_pact::*;

// 1. Auditor or client defines the agreed contract
let contract = spring_pact::new("Auditoría Salarial 2026")
    |> expect_filter(col("age") >= 18)
    |> expect_na_max("salary", 0.03)
    |> expect_outlier_rule("salary", method = "iqr", factor = 3.0)
    |> expect_vif_max(5.0)
    |> expect_min_sample_size(1000)
    |> expect_vcov_kind("HC3");

// Immutable contract protocol digest
let contract_hash = contract.digest();
println("Protocol Hash: {}", contract_hash);

// 2. Analyst ingests data and enforces contract rules
let raw_data = read_csv("data/empresa_nomina.csv");
let clean_data = raw_data |> spring_pact::enforce_data_rules(contract);

// 3. Econometric model estimation
let fit = ols(salary ~ education + experience + gender, clean_data);

// 4. Comprehensive verification and certificate issuance
let receipt = contract |> verify(fit);

if (receipt.is_compliant()) {
    println("/ᐠ˵- ⩊ -˵マ ✧ spring_pact protocol verified!");
    receipt.render_cockpit();
    receipt.save_certificate("audit_receipt.json");
} else {
    println("≽(◉˕ ◉ ≼マ Contract violated. Detected discrepancies:");
    receipt.print_violations();
};
```

---

## 5. Salida Cockpit Deck (UX con Haru)

El recibo genera un panel formateado con la estética de **Cockpit Deck**:

```text
┌─ [GHL Cockpit Deck] spring_pact Compliance Certificate ─────── [VERIFIED] ─┐
│ Contract Name: Auditoría Salarial 2026                                    │
│ Contract Hash: sha256:7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d6...    │
│ Dataset Hash:  sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b...    │
├───────────────────────────────────────────────────────────────────────────┤
│ Rule                                Expected          Observed     Status │
│ • Age range filter                  >= 18             min = 19     PASS   │
│ • Salary NA threshold               <= 3.0%           0.4%         PASS   │
│ • Predictor Multicollinearity (VIF) < 5.0             max = 2.1    PASS   │
│ • Effective Sample Size (N)         >= 1000           N = 12,450   PASS   │
│ • Standard Error Covariance Matrix  HC3 Robust        HC3 Applied  PASS   │
├───────────────────────────────────────────────────────────────────────────┤
│ /ᐠ˵- ⩊ -˵マ ✧ All statistical pact expectations certified and verified!    │
└───────────────────────────────────────────────────────────────────────────┘
```
