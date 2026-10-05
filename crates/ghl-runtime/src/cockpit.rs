//! First-class Cockpit Deck native primitives for GHL scripts and packages (RFC 14).
//!
//! Exposes `CockpitPanel`, line/KV builders, dividers, badges, and renderers
//! directly to the runtime so packages like `spring_pact` and `ghl_causal` can
//! render rich, responsive terminal cards without hardcoded string formatting.

use ghl_diagnostics::{
    finish_progress, update_progress_bar, update_progress_spinner, CockpitPanel, Diagnostic,
    ProgressTheme, RenderCaps, Sparkline, SpinnerStyle,
};
use crate::value::Value;

/// Creates a new `CockpitPanel` card with an optional badge.
///
/// Signature: `cockpit(title: String, [badge: String]) -> CockpitPanel`
pub fn native_cockpit(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let title_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cockpit()` requires at least 1 argument (title)")
    })?;

    let title = match title_val {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    let mut panel = CockpitPanel::new(title);

    if let Some(badge_val) = args.get(1) {
        let badge = match badge_val {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        panel.with_badge(badge);
    }

    Ok(Value::CockpitPanel(Box::new(panel)))
}

/// Sets or updates the badge on an existing `CockpitPanel`.
///
/// Signature: `cockpit_with_badge(panel: CockpitPanel, badge: String) -> CockpitPanel`
pub fn native_cockpit_with_badge(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error("C0201", "`cockpit_with_badge()` requires 2 arguments: (panel, badge)"));
    }

    let panel = match &args[0] {
        Value::CockpitPanel(p) => (**p).clone(),
        other => return Err(Diagnostic::compute_error("C0202", format!("`cockpit_with_badge()` expects a CockpitPanel, found `{}`", other.type_name()))),
    };

    let badge = match &args[1] {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    let mut panel = panel;
    panel.with_badge(badge);
    Ok(Value::CockpitPanel(Box::new(panel)))
}

/// Adds a key-value metric line to a `CockpitPanel`.
///
/// Signature: `cockpit_add_kv(panel: CockpitPanel, key: Any, val: Any) -> CockpitPanel`
pub fn native_cockpit_add_kv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error("C0201", "`cockpit_add_kv()` requires 3 arguments: (panel, key, val)"));
    }

    let panel = match &args[0] {
        Value::CockpitPanel(p) => (**p).clone(),
        other => return Err(Diagnostic::compute_error("C0202", format!("`cockpit_add_kv()` expects a CockpitPanel, found `{}`", other.type_name()))),
    };

    let key = match &args[1] {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    let val = match &args[2] {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    let mut panel = panel;
    panel.add_kv(key, val);
    Ok(Value::CockpitPanel(Box::new(panel)))
}

/// Adds a free-form line of text or table row to a `CockpitPanel`.
///
/// Signature: `cockpit_add_line(panel: CockpitPanel, line: Any) -> CockpitPanel`
pub fn native_cockpit_add_line(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error("C0201", "`cockpit_add_line()` requires 2 arguments: (panel, line)"));
    }

    let panel = match &args[0] {
        Value::CockpitPanel(p) => (**p).clone(),
        other => return Err(Diagnostic::compute_error("C0202", format!("`cockpit_add_line()` expects a CockpitPanel, found `{}`", other.type_name()))),
    };

    let line = match &args[1] {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    let mut panel = panel;
    panel.add_line(line);
    Ok(Value::CockpitPanel(Box::new(panel)))
}

/// Adds a horizontal dividing border to a `CockpitPanel`.
///
/// Signature: `cockpit_add_divider(panel: CockpitPanel) -> CockpitPanel`
pub fn native_cockpit_add_divider(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let panel_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cockpit_add_divider()` requires 1 argument: (panel)")
    })?;

    let panel = match panel_val {
        Value::CockpitPanel(p) => (**p).clone(),
        other => return Err(Diagnostic::compute_error("C0202", format!("`cockpit_add_divider()` expects a CockpitPanel, found `{}`", other.type_name()))),
    };

    let mut panel = panel;
    panel.add_divider();
    Ok(Value::CockpitPanel(Box::new(panel)))
}

/// Renders a `CockpitPanel` (or any value) directly to the terminal stdout respecting terminal caps.
///
/// Signature: `render_cockpit(panel: Any) -> ()` (aliased as `show_cockpit` and `cockpit_render`)
pub fn native_render_cockpit(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let caps = RenderCaps::detect();
    for val in &args {
        match val {
            Value::CockpitPanel(p) => println!("{}", p.render(&caps)),
            other => println!("{}", other.render_styled(&caps)),
        }
    }
    Ok(Value::Unit)
}

/// Formats a `CockpitPanel` (or any value) into a rendered String respecting terminal caps.
///
/// Signature: `format_cockpit(panel: Any) -> String` (aliased as `cockpit_format`)
pub fn native_format_cockpit(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let caps = RenderCaps::detect();
    let val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`format_cockpit()` requires 1 argument: (panel)")
    })?;

    let text = match val {
        Value::CockpitPanel(p) => p.render(&caps),
        other => other.render_styled(&caps),
    };

    Ok(Value::String(text))
}

/// Renders a sequence of numeric values into an inline distribution sparkline string (` ▂▃▄▅▆▇█`).
///
/// Signature: `sparkline(values: Vector, [max_len: i64]) -> String` (aliased as `cockpit_sparkline`)
pub fn native_sparkline(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`sparkline()` requires at least 1 argument (vector)")
    })?;

    let values: Vec<f64> = match val {
        Value::Vector(vd) => vd.iter().filter_map(|v| v.as_f64()).collect(),
        _ => return Err(Diagnostic::compute_error("C0202", format!("`sparkline()` expects a numeric Vector, found `{}`", val.type_name()))),
    };

    let max_len = args.get(1).and_then(|v| v.as_i64()).map(|n| n.max(1) as usize);
    let caps = RenderCaps::detect();
    let rendered = Sparkline::render(&values, max_len, &caps);
    Ok(Value::String(rendered))
}

/// Updates an in-place single-line progress bar with telemetry and color theme.
///
/// Signature: `progress_bar(current: Int, total: Int, [label: String, details: String, theme: String]) -> ()`
pub fn native_progress_bar(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`progress_bar()` requires at least 2 arguments: (current, total, [label, details, theme])",
        ));
    }

    let current = args[0]
        .as_i64()
        .or_else(|| args[0].as_f64().map(|f| f as i64))
        .unwrap_or(0);
    let total = args[1]
        .as_i64()
        .or_else(|| args[1].as_f64().map(|f| f as i64))
        .unwrap_or(1);

    let label = args.get(2).map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_default();

    let details = args.get(3).map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_default();

    let theme_str = args.get(4).map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_else(|| "cyan".to_string());

    let theme = ProgressTheme::parse(&theme_str);
    let caps = RenderCaps::detect();

    update_progress_bar(current, total, &label, &details, theme, &caps);
    Ok(Value::Unit)
}

/// Updates an in-place indeterminate spinner with animation and color theme.
///
/// Signature: `progress_spinner(label: String, [details: String, style: String, theme: String]) -> ()`
pub fn native_progress_spinner(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let label = args.first().map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_default();

    let details = args.get(1).map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_default();

    let style_str = args.get(2).map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_else(|| "dots".to_string());

    let theme_str = args.get(3).map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_else(|| "cyan".to_string());

    let style = SpinnerStyle::parse(&style_str);
    let theme = ProgressTheme::parse(&theme_str);
    let caps = RenderCaps::detect();

    update_progress_spinner(&label, &details, style, theme, &caps);
    Ok(Value::Unit)
}

/// Finalizes the active progress bar or spinner with an optional message.
///
/// Signature: `progress_done([message: String]) -> ()`
pub fn native_progress_done(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let message = args.first().map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_default();

    let caps = RenderCaps::detect();
    finish_progress(&message, &caps);
    Ok(Value::Unit)
}

/// Adds a progress bar component to an existing `CockpitPanel`.
///
/// Signature: `cockpit_add_progress(panel: CockpitPanel, current: Int, total: Int, [label: String, theme: String]) -> CockpitPanel`
pub fn native_cockpit_add_progress(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`cockpit_add_progress()` requires at least 3 arguments: (panel, current, total, [label, theme])",
        ));
    }

    let panel = match &args[0] {
        Value::CockpitPanel(p) => (**p).clone(),
        other => return Err(Diagnostic::compute_error("C0202", format!("`cockpit_add_progress()` expects a CockpitPanel, found `{}`", other.type_name()))),
    };

    let current = args[1]
        .as_i64()
        .or_else(|| args[1].as_f64().map(|f| f as i64))
        .unwrap_or(0);
    let total = args[2]
        .as_i64()
        .or_else(|| args[2].as_f64().map(|f| f as i64))
        .unwrap_or(1);

    let label = args.get(3).map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_default();

    let theme = args.get(4).map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_else(|| "cyan".to_string());

    let mut panel = panel;
    panel.add_progress(label, current, total, theme);
    Ok(Value::CockpitPanel(Box::new(panel)))
}

/// Adds a compact circular progress disc to an existing `CockpitPanel`.
///
/// Signature: `cockpit_add_circle(panel: CockpitPanel, current: Int, total: Int, [label: String, theme: String]) -> CockpitPanel`
pub fn native_cockpit_add_circle(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`cockpit_add_circle()` requires at least 3 arguments: (panel, current, total, [label, theme])",
        ));
    }

    let panel = match &args[0] {
        Value::CockpitPanel(p) => (**p).clone(),
        other => return Err(Diagnostic::compute_error("C0202", format!("`cockpit_add_circle()` expects a CockpitPanel, found `{}`", other.type_name()))),
    };

    let current = args[1]
        .as_i64()
        .or_else(|| args[1].as_f64().map(|f| f as i64))
        .unwrap_or(0);
    let total = args[2]
        .as_i64()
        .or_else(|| args[2].as_f64().map(|f| f as i64))
        .unwrap_or(1);

    let label = args.get(3).map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_default();

    let theme = args.get(4).map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).unwrap_or_else(|| "cyan".to_string());

    let mut panel = panel;
    panel.add_circle(label, current, total, theme);
    Ok(Value::CockpitPanel(Box::new(panel)))
}
