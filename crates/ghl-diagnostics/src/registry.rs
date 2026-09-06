//! Semantic symbol registry for GHL Cockpit Deck.
//!
//! Provides canonical event codes, Kaomojis (Gojo/Haru/NEKO), and ASCII fallbacks.

use crate::caps::RenderCaps;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticCode {
    // Compute glitches / Gojo
    ComputeError,
    ComputeSyntax,
    ComputeType,
    ComputeRuntime,

    // Statistical feline / NEKO
    StatError,
    StatWarning,
    StatSingular,
    StatCollinear,
    StatDropout,
    StatSampleSize,

    // Execution / Haru
    ExecSuccess,
    ExecInfo,
    ExecRunning,
    PipelineStep,
}

pub struct SymbolEntry {
    pub unicode: &'static str,
    pub ascii: &'static str,
    pub badge: &'static str,
}

impl SemanticCode {
    pub fn entry(&self) -> SymbolEntry {
        match self {
            Self::ComputeError => SymbolEntry {
                unicode: "(ノ°□°)ノ",
                ascii: "[COMP-ERR]",
                badge: "COMPUTE ERROR",
            },
            Self::ComputeSyntax => SymbolEntry {
                unicode: "(ノಠ益ಠ)ノ彡┻━┻",
                ascii: "[SYNTAX-ERR]",
                badge: "SYNTAX ERROR",
            },
            Self::ComputeType => SymbolEntry {
                unicode: "(⊙_☉)",
                ascii: "[TYPE-ERR]",
                badge: "TYPE MISMATCH",
            },
            Self::ComputeRuntime => SymbolEntry {
                unicode: "(ノಠ_ಠ)ノ",
                ascii: "[RTIME-ERR]",
                badge: "RUNTIME ERROR",
            },

            Self::StatError => SymbolEntry {
                unicode: "ฅ(ﾐΦ ﻌ Φﾐ)ฅ",
                ascii: "[STAT-ERR]",
                badge: "STATISTICAL ERROR",
            },
            Self::StatWarning => SymbolEntry {
                unicode: "(ФωФ)",
                ascii: "[STAT-WARN]",
                badge: "STATISTICAL WARNING",
            },
            Self::StatSingular => SymbolEntry {
                unicode: "[| | 0] ฅ(ﾐΦ ﻌ Φﾐ)ฅ",
                ascii: "[SINGULAR]",
                badge: "SINGULAR MATRIX",
            },
            Self::StatCollinear => SymbolEntry {
                unicode: "( ;¬_¬) VIF>10",
                ascii: "[VIF>10]",
                badge: "MULTICOLLINEARITY",
            },
            Self::StatDropout => SymbolEntry {
                unicode: "(=ｘェｘ=) NA",
                ascii: "[NA-DROP]",
                badge: "DATA DROPOUT",
            },
            Self::StatSampleSize => SymbolEntry {
                unicode: "(ФωФ) N<5",
                ascii: "[WARN-N]",
                badge: "SMALL SAMPLE",
            },

            Self::ExecSuccess => SymbolEntry {
                unicode: "(U・ᴥ・U) ✧",
                ascii: "[SUCCESS]",
                badge: "SUCCESS",
            },
            Self::ExecInfo => SymbolEntry {
                unicode: "(=^･ω･^=)",
                ascii: "[INFO]",
                badge: "NOTE",
            },
            Self::ExecRunning => SymbolEntry {
                unicode: "(ง'̀-'́)ง ▶",
                ascii: "[RUN]",
                badge: "RUNNING",
            },
            Self::PipelineStep => SymbolEntry {
                unicode: "✔",
                ascii: "OK",
                badge: "STEP",
            },
        }
    }

    /// Retrieve the appropriate glyph given terminal capabilities.
    pub fn glyph(&self, caps: &RenderCaps) -> &'static str {
        let entry = self.entry();
        if caps.unicode_enabled {
            entry.unicode
        } else {
            entry.ascii
        }
    }

    /// Format a styled badge tag, e.g. `[COMPUTE ERROR]` in red.
    pub fn badge(&self, caps: &RenderCaps) -> String {
        let entry = self.entry();
        let text = format!("[{}]", entry.badge);
        match self {
            Self::ComputeError | Self::ComputeSyntax | Self::ComputeType | Self::ComputeRuntime => {
                caps.red(&caps.bold(&text))
            }
            Self::StatError | Self::StatSingular => caps.red(&caps.bold(&text)),
            Self::StatWarning | Self::StatCollinear | Self::StatSampleSize => {
                caps.yellow(&caps.bold(&text))
            }
            Self::StatDropout => caps.yellow(&text),
            Self::ExecSuccess | Self::PipelineStep => caps.green(&caps.bold(&text)),
            Self::ExecInfo => caps.cyan(&text),
            Self::ExecRunning => caps.magenta(&caps.bold(&text)),
        }
    }
}

pub struct SymbolRegistry;

impl SymbolRegistry {
    pub fn lookup(code: SemanticCode, caps: &RenderCaps) -> String {
        format!("{} {}", code.glyph(caps), code.badge(caps))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_unicode_and_ascii() {
        let rich = RenderCaps::rich_terminal(80);
        let plain = RenderCaps::ascii_plain(80);

        assert_eq!(SemanticCode::StatError.glyph(&rich), "ฅ(ﾐΦ ﻌ Φﾐ)ฅ");
        assert_eq!(SemanticCode::StatError.glyph(&plain), "[STAT-ERR]");

        assert_eq!(SemanticCode::ExecSuccess.glyph(&rich), "(U・ᴥ・U) ✧");
        assert_eq!(SemanticCode::ExecSuccess.glyph(&plain), "[SUCCESS]");
    }
}
