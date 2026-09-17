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

    // Rich Feline Expressions (Gojo & Haru)
    ApprovalGojo,
    ApprovalHaru,
    DocQuery,
    ExecIdle,
    PlotSuccess,
    GojoBanner,
    HaruWave,
    NyaLoaf,
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
                unicode: "/ᐠ ¬`‸´¬ マ",
                ascii: "[COMP-ERR]",
                badge: "COMPUTE ERROR",
            },
            Self::ComputeSyntax => SymbolEntry {
                unicode: "/ᐠ ¬`‸´¬ マ",
                ascii: "[SYNTAX-ERR]",
                badge: "SYNTAX ERROR",
            },
            Self::ComputeType => SymbolEntry {
                unicode: "/ᐠ ¬`‸´¬ マ",
                ascii: "[TYPE-ERR]",
                badge: "TYPE MISMATCH",
            },
            Self::ComputeRuntime => SymbolEntry {
                unicode: "/ᐠ ¬`‸´¬ マ",
                ascii: "[RTIME-ERR]",
                badge: "RUNTIME ERROR",
            },

            Self::StatError => SymbolEntry {
                unicode: "≽(◉˕ ◉ ≼マ",
                ascii: "[STAT-ERR]",
                badge: "STATISTICAL ERROR",
            },
            Self::StatWarning => SymbolEntry {
                unicode: "/ᐠ ·•᷄ ˕ •᷅マ",
                ascii: "[STAT-WARN]",
                badge: "STATISTICAL WARNING",
            },
            Self::StatSingular => SymbolEntry {
                unicode: "[| | 0] ≽(◉˕ ◉ ≼マ",
                ascii: "[SINGULAR]",
                badge: "SINGULAR MATRIX",
            },
            Self::StatCollinear => SymbolEntry {
                unicode: "≽(◉˕ ◉ ≼マ VIF>10",
                ascii: "[VIF>10]",
                badge: "MULTICOLLINEARITY",
            },
            Self::StatDropout => SymbolEntry {
                unicode: "/ᐠ ◞ ᆺ ◟マ NA",
                ascii: "[NA-DROP]",
                badge: "DATA DROPOUT",
            },
            Self::StatSampleSize => SymbolEntry {
                unicode: "≽(◉˕ ◉ ≼マ N<5",
                ascii: "[WARN-N]",
                badge: "SMALL SAMPLE",
            },

            Self::ExecSuccess => SymbolEntry {
                unicode: "/ᐠ˵- ⩊ -˵マ ✧",
                ascii: "[SUCCESS]",
                badge: "SUCCESS",
            },
            Self::ExecInfo => SymbolEntry {
                unicode: "ฅ(•⩊ •マ",
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

            Self::ApprovalGojo => SymbolEntry {
                unicode: "ദ്ദി(ᓀ‸ᓂマ ੭",
                ascii: "[OK]",
                badge: "APPROVED",
            },
            Self::ApprovalHaru => SymbolEntry {
                unicode: "ദ്ദി/ᐠ - ⩊ -マ",
                ascii: "[OK]",
                badge: "PASSED",
            },
            Self::DocQuery => SymbolEntry {
                unicode: "/ᐠ • ˕ •マ ?",
                ascii: "[?]",
                badge: "HELP",
            },
            Self::ExecIdle => SymbolEntry {
                unicode: "ㅤ/ᐠ - ˕ -マᶻ 𝗓 𐰁",
                ascii: "[IDLE]",
                badge: "IDLE",
            },
            Self::PlotSuccess => SymbolEntry {
                unicode: "/ ˵> ˕ <˵マ",
                ascii: "[PLOT]",
                badge: "RENDERED",
            },
            Self::GojoBanner => SymbolEntry {
                unicode: "𝑴𝒆𝒐𝒘. ฅ(•- •マ",
                ascii: "[GHL]",
                badge: "SYSTEM",
            },
            Self::HaruWave => SymbolEntry {
                unicode: "ฅ(^⩊ •マ",
                ascii: "[TIP]",
                badge: "TIP",
            },
            Self::NyaLoaf => SymbolEntry {
                unicode: "/ᐠ_ ꞈ _ᐟ\\ɴʏᴀ~",
                ascii: "[NYA]",
                badge: "NYA",
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

    /// Retrieve the formatted badge with appropriate color styling.
    pub fn badge(&self, caps: &RenderCaps) -> String {
        let text = format!("[{}]", self.entry().badge);
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
            Self::ApprovalGojo => caps.gojo(&caps.bold(&text)),
            Self::ApprovalHaru | Self::PlotSuccess | Self::HaruWave => caps.haru(&caps.bold(&text)),
            Self::DocQuery => caps.cyan(&caps.bold(&text)),
            Self::ExecIdle | Self::NyaLoaf => caps.dim(&text),
            Self::GojoBanner => caps.gojo(&caps.bold(&text)),
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

        assert_eq!(SemanticCode::StatError.glyph(&rich), "≽(◉˕ ◉ ≼マ");
        assert_eq!(SemanticCode::StatError.glyph(&plain), "[STAT-ERR]");

        assert_eq!(SemanticCode::ExecSuccess.glyph(&rich), "/ᐠ˵- ⩊ -˵マ ✧");
        assert_eq!(SemanticCode::ExecSuccess.glyph(&plain), "[SUCCESS]");

        assert_eq!(SemanticCode::ApprovalGojo.glyph(&rich), "ദ്ദി(ᓀ‸ᓂマ ੭");
        assert_eq!(SemanticCode::ApprovalHaru.glyph(&rich), "ദ്ദി/ᐠ - ⩊ -マ");
        assert_eq!(SemanticCode::DocQuery.glyph(&rich), "/ᐠ • ˕ •マ ?");
        assert_eq!(SemanticCode::PlotSuccess.glyph(&rich), "/ ˵> ˕ <˵マ");
    }
}
