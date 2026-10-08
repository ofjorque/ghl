//! Accessible and publication-grade color palettes for GHL graphics.
//!
//! Provides:
//! - Okabe-Ito palette (Colorblind-safe universal default: Wong 2011 / Okabe & Ito 2008)
//! - Viridis sequential perceptually uniform palette
//! - ColorBrewer qualitative palettes (Dark2, Set1)
//! - Terminal ANSI color mapping and Plotters RGBColor conversion

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaletteColor {
    pub name: &'static str,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub hex: &'static str,
    pub ansi_code: &'static str,
}

impl PaletteColor {
    pub const fn new(
        name: &'static str,
        r: u8,
        g: u8,
        b: u8,
        hex: &'static str,
        ansi_code: &'static str,
    ) -> Self {
        Self {
            name,
            r,
            g,
            b,
            hex,
            ansi_code,
        }
    }

    pub fn to_rgb_tuple(&self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    pub fn to_plotters_color(&self) -> plotters::style::RGBColor {
        plotters::style::RGBColor(self.r, self.g, self.b)
    }
}

/// Okabe-Ito 8-color universal palette (optimized for all color-vision deficiencies).
pub static OKABE_ITO: [PaletteColor; 8] = [
    PaletteColor::new("Sky Blue", 86, 180, 233, "#56B4E9", "\x1b[38;2;86;180;233m"),
    PaletteColor::new("Orange", 230, 159, 0, "#E69F00", "\x1b[38;2;230;159;0m"),
    PaletteColor::new(
        "Bluish Green",
        0,
        158,
        115,
        "#009E73",
        "\x1b[38;2;0;158;115m",
    ),
    PaletteColor::new("Vermilion", 213, 94, 0, "#D55E00", "\x1b[38;2;213;94;0m"),
    PaletteColor::new(
        "Reddish Purple",
        204,
        121,
        167,
        "#CC79A7",
        "\x1b[38;2;204;121;167m",
    ),
    PaletteColor::new("Yellow", 240, 228, 66, "#F0E442", "\x1b[38;2;240;228;66m"),
    PaletteColor::new("Blue", 0, 114, 178, "#0072B2", "\x1b[38;2;0;114;178m"),
    PaletteColor::new("Black", 0, 0, 0, "#000000", "\x1b[38;2;0;0;0m"),
];

/// Dark2 Qualitative palette (ColorBrewer).
pub static DARK2: [PaletteColor; 8] = [
    PaletteColor::new("Teal", 27, 158, 119, "#1B9E77", "\x1b[38;2;27;158;119m"),
    PaletteColor::new("Orange", 217, 95, 2, "#D95F02", "\x1b[38;2;217;95;2m"),
    PaletteColor::new("Purple", 117, 112, 179, "#7570B3", "\x1b[38;2;117;112;179m"),
    PaletteColor::new("Magenta", 231, 41, 138, "#E7298A", "\x1b[38;2;231;41;138m"),
    PaletteColor::new("Lime", 102, 166, 30, "#66A61E", "\x1b[38;2;102;166;30m"),
    PaletteColor::new("Gold", 230, 171, 2, "#E6AB02", "\x1b[38;2;230;171;2m"),
    PaletteColor::new("Brown", 166, 118, 29, "#A6761D", "\x1b[38;2;166;118;29m"),
    PaletteColor::new("Gray", 102, 102, 102, "#666666", "\x1b[38;2;102;102;102m"),
];

/// Viridis sequential colormap sample points (6 steps).
pub static VIRIDIS: [PaletteColor; 6] = [
    PaletteColor::new("Viridis-1", 68, 1, 84, "#440154", "\x1b[38;2;68;1;84m"),
    PaletteColor::new("Viridis-2", 65, 68, 135, "#414487", "\x1b[38;2;65;68;135m"),
    PaletteColor::new(
        "Viridis-3",
        42,
        120,
        142,
        "#2A788E",
        "\x1b[38;2;42;120;142m",
    ),
    PaletteColor::new(
        "Viridis-4",
        34,
        168,
        132,
        "#22A884",
        "\x1b[38;2;34;168;132m",
    ),
    PaletteColor::new(
        "Viridis-5",
        122,
        209,
        81,
        "#7AD151",
        "\x1b[38;2;122;209;81m",
    ),
    PaletteColor::new(
        "Viridis-6",
        253,
        231,
        37,
        "#FDE725",
        "\x1b[38;2;253;231;37m",
    ),
];

/// Retrieve the i-th color from the default Okabe-Ito palette, cycling if necessary.
pub fn get_okabe_ito_color(index: usize) -> &'static PaletteColor {
    &OKABE_ITO[index % OKABE_ITO.len()]
}

/// Retrieve the i-th color from the Dark2 palette.
pub fn get_dark2_color(index: usize) -> &'static PaletteColor {
    &DARK2[index % DARK2.len()]
}

/// Sample viridis colormap at normalized ratio t in [0.0, 1.0].
pub fn sample_viridis(t: f64) -> (u8, u8, u8) {
    let t_clamped = t.clamp(0.0, 1.0);
    let idx = (t_clamped * (VIRIDIS.len() - 1) as f64).round() as usize;
    VIRIDIS[idx].to_rgb_tuple()
}
