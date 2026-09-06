//! Diagnostic reporting engine for GHL (Generalized Hypothesis Language).
//!
//! Provides the dual error classification:
//! - `[Compute Error Cxxxx]` with expressive computer-glitch Kaomojis (e.g. `(ノ°□°)ノ`)
//! - `[Statistical Error Sxxxx]` with observant feline Kaomojis (e.g. `ฅ(ﾐΦ ﻌ Φﾐ)ฅ`)
//! - `[Statistical Warning SWxxxx]` with cautionary feline Kaomojis (e.g. `(ФωФ)`)
//! - Success / Hints with faithful canine Haru Kaomojis (e.g. `(U・ᴥ・U)`)

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

#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
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

    pub fn render(&self) -> String {
        let mut out = String::new();
        let kaomoji = self.severity.kaomoji();
        let prefix = self.severity.prefix();

        out.push_str(&format!("{kaomoji} [{prefix} {}]: {}\n", self.code, self.message));
        if let Some(loc) = &self.location {
            out.push_str(&format!("  ┌─ {}:{}:{}\n", loc.file, loc.line, loc.column));
        }
        if let Some(help) = &self.help {
            out.push_str(&format!("  = Help: {}\n", help));
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

        let rendered = diag.render();
        assert!(rendered.contains("ฅ(ﾐΦ ﻌ Φﾐ)ฅ"));
        assert!(rendered.contains("[Statistical Error S0301]"));
        assert!(rendered.contains("model.gh:42:10"));
    }

    #[test]
    fn test_compute_error_render() {
        let diag = Diagnostic::compute_error("C0102", "Type mismatch in assignment")
            .with_location("main.gh", 12, 5)
            .with_help("Explicitly convert to expected type.");

        let rendered = diag.render();
        assert!(rendered.contains("(ノ°□°)ノ"));
        assert!(rendered.contains("[Compute Error C0102]"));
    }
}

