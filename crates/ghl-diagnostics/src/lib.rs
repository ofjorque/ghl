//! Diagnostic reporting engine for GHL (Generalized Hypothesis Language).
//!
//! Provides the dual error classification and Cockpit Deck telemetry integration:
//! - `[Compute Error Cxxxx]` with expressive computer-glitch Kaomojis (e.g. `(ノ°□°)ノ`)
//! - `[Statistical Error Sxxxx]` with observant feline Kaomojis (e.g. `ฅ(ﾐΦ ﻌ Φﾐ)ฅ`)
//! - `[Statistical Warning SWxxxx]` with cautionary feline Kaomojis (e.g. `(ФωФ)`)
//! - Success / Hints with faithful canine Haru Kaomojis (e.g. `(U・ᴥ・U)`)
//!
//! Fully integrates with Cockpit Deck v0.5.0:
//! - `RenderCaps` capability detection and degradation matrix (ANSI, Unicode, Width, NO_COLOR)
//! - `SymbolRegistry` semantic codes and badges
//! - `CockpitPanel` width-clipped terminal cards
//! - `Sparkline` 8-level inline data distributions

pub mod caps;
pub mod panel;
pub mod registry;
pub mod sparkline;

pub use caps::RenderCaps;
pub use panel::CockpitPanel;
pub use registry::{SemanticCode, SymbolEntry, SymbolRegistry};
pub use sparkline::Sparkline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    ComputeError,
    StatisticalError,
    StatisticalWarning,
    Info,
    Success,
}

impl DiagnosticSeverity {
    pub fn kaomoji(&self) -> &'static str {
        match self {
            Self::ComputeError => "(ノ°□°)ノ",
            Self::StatisticalError => "ฅ(ﾐΦ ﻌ Φﾐ)ฅ",
            Self::StatisticalWarning => "(ФωФ)",
            Self::Info => "(=^･ω･^=)",
            Self::Success => "(U・ᴥ・U)",
        }
    }

    pub fn ascii_fallback(&self) -> &'static str {
        match self {
            Self::ComputeError => "[COMP-ERR]",
            Self::StatisticalError => "[STAT-ERR]",
            Self::StatisticalWarning => "[STAT-WARN]",
            Self::Info => "[NOTE]",
            Self::Success => "[SUCCESS]",
        }
    }

    pub fn prefix(&self) -> &'static str {
        match self {
            Self::ComputeError => "Compute Error",
            Self::StatisticalError => "Statistical Error",
            Self::StatisticalWarning => "Statistical Warning",
            Self::Info => "Note",
            Self::Success => "Success",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub location: Option<SourceLocation>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn compute_error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::ComputeError,
            code: code.into(),
            message: message.into(),
            location: None,
            help: None,
        }
    }

    pub fn statistical_error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::StatisticalError,
            code: code.into(),
            message: message.into(),
            location: None,
            help: None,
        }
    }

    pub fn statistical_warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::StatisticalWarning,
            code: code.into(),
            message: message.into(),
            location: None,
            help: None,
        }
    }

    pub fn with_location(mut self, file: impl Into<String>, line: usize, column: usize) -> Self {
        self.location = Some(SourceLocation {
            file: file.into(),
            line,
            column,
        });
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn is_error(&self) -> bool {
        matches!(
            self.severity,
            DiagnosticSeverity::ComputeError | DiagnosticSeverity::StatisticalError
        )
    }

    pub fn is_warning(&self) -> bool {
        matches!(self.severity, DiagnosticSeverity::StatisticalWarning)
    }

    /// Render diagnostic using default environment capabilities.
    pub fn render(&self) -> String {
        self.render_with_caps(&RenderCaps::detect())
    }

    /// Render diagnostic with explicit terminal capabilities (Unicode vs ASCII, Color vs NO_COLOR).
    pub fn render_with_caps(&self, caps: &RenderCaps) -> String {
        let mut out = String::new();
        let kaomoji = if caps.unicode_enabled {
            self.severity.kaomoji()
        } else {
            self.severity.ascii_fallback()
        };
        let prefix = self.severity.prefix();
        let header = format!("{kaomoji} [{prefix} {}]: {}", self.code, self.message);

        let styled_header = match self.severity {
            DiagnosticSeverity::ComputeError | DiagnosticSeverity::StatisticalError => {
                caps.red(&caps.bold(&header))
            }
            DiagnosticSeverity::StatisticalWarning => caps.yellow(&caps.bold(&header)),
            DiagnosticSeverity::Info => caps.cyan(&header),
            DiagnosticSeverity::Success => caps.green(&caps.bold(&header)),
        };

        out.push_str(&format!("{styled_header}\n"));

        let tree_corner = if caps.unicode_enabled { "┌─" } else { "+-" };

        if let Some(loc) = &self.location {
            let loc_str = format!("  {tree_corner} {}:{}:{}", loc.file, loc.line, loc.column);
            out.push_str(&format!("{}\n", caps.dim(&loc_str)));
        }
        if let Some(help) = &self.help {
            let help_str = format!("  = Help: {help}");
            out.push_str(&format!("{}\n", caps.dim(&help_str)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statistical_error_render() {
        let diag = Diagnostic::statistical_error("S0301", "Singular covariance matrix")
            .with_location("model.gh", 42, 10)
            .with_help("Consider Ridge regularization or dropping collinear predictors.");

        let rendered = diag.render_with_caps(&RenderCaps::rich_terminal(80));
        assert!(rendered.contains("ฅ(ﾐΦ ﻌ Φﾐ)ฅ"));
        assert!(rendered.contains("[Statistical Error S0301]"));
        assert!(rendered.contains("model.gh:42:10"));
    }

    #[test]
    fn test_compute_error_render() {
        let diag = Diagnostic::compute_error("C0102", "Type mismatch in assignment")
            .with_location("main.gh", 12, 5)
            .with_help("Explicitly convert to expected type.");

        let rendered = diag.render_with_caps(&RenderCaps::rich_terminal(80));
        assert!(rendered.contains("(ノ°□°)ノ"));
        assert!(rendered.contains("[Compute Error C0102]"));
    }

    #[test]
    fn test_ascii_degradation_render() {
        let diag = Diagnostic::statistical_warning("SW0301", "Multicollinearity detected")
            .with_location("analysis.gh", 15, 3)
            .with_help("VIF > 10. Consider dropping correlated columns.");

        let rendered = diag.render_with_caps(&RenderCaps::ascii_plain(80));
        assert!(rendered.contains("[STAT-WARN]"));
        assert!(rendered.contains("+- analysis.gh:15:3"));
        assert!(!rendered.contains("\x1b["));
    }
}
