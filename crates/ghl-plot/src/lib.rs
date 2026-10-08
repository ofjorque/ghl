//! GHL Grammar of Graphics Engine (`ghl-plot`).
//!
//! Provides a reactive, typed, multi-layer grammar of graphics engine grounded
//! in GHL's native type system (RFC 16).
//!
//! Three rendering targets:
//! - **Native**: High-DPI raster (PNG) and vector (SVG) via Plotters with multi-layer composition
//!   and colorblind-safe Okabe-Ito palettes.
//! - **Terminal**: Cockpit Deck telemetry cards with 8-level blocks (` ▂▃▄▅▆▇█`), ANSI colors,
//!   and Tukey boxplots.
//! - **Vega**: Interactive Vega-Lite v5 specification JSON for Positron, Jupyter, and web tools.

pub mod fonts;
pub mod palette;
pub mod render_native;
pub mod render_terminal;
pub mod render_vega;
pub mod spec;

pub use fonts::{discover_and_register_font, os_font_directories};
pub use palette::{
    DARK2, OKABE_ITO, PaletteColor, VIRIDIS, get_dark2_color, get_okabe_ito_color, sample_viridis,
};
pub use render_native::NativeRenderer;
pub use render_terminal::TerminalRenderer;
pub use render_vega::VegaRenderer;
pub use spec::{
    AestheticMap, CompositePlot, DataSeries, FacetLayout, FacetPanel, FacetScales, FacetSpec,
    FiveNumberSummary, GeomKind, GeomLayer, HistogramBins, LayerData, LinearFit, MarkerShape,
    PlotLabels, PlotSpec, PlotTheme, ScaleModifier, ScaleTransform, ThemeModifier,
};

impl PlotSpec {
    /// Render the plot into an ASCII/Unicode Cockpit Deck terminal card.
    pub fn render(&self, caps: &ghl_diagnostics::RenderCaps) -> String {
        TerminalRenderer::render(self, caps)
    }

    /// Export the plot to a raster (PNG) or vector (SVG) image file with 800x600 resolution.
    pub fn save_file(&self, path: &str) -> Result<(), String> {
        NativeRenderer::save_file(self, path)
    }

    /// Export the plot to an image file with custom dimensions.
    pub fn save_file_with_size(&self, path: &str, width: u32, height: u32) -> Result<(), String> {
        NativeRenderer::save_file_with_size(self, path, width, height)
    }

    /// Render the plot directly into an SVG string.
    pub fn to_svg(&self, width: u32, height: u32) -> Result<String, String> {
        NativeRenderer::to_svg(self, width, height)
    }

    /// Export the plot as an interactive Vega-Lite v5 JSON string.
    pub fn to_vega_json(&self) -> Result<String, String> {
        VegaRenderer::to_vega_json(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_layer_spec_creation() {
        let spec = PlotSpec::new()
            .with_title("Engine Displacement vs MPG")
            .with_labels(
                Some("MTcars".into()),
                Some("Displacement".into()),
                Some("MPG".into()),
            )
            .with_xy_data(
                vec![160.0, 160.0, 108.0, 258.0],
                vec![21.0, 21.0, 22.8, 21.4],
            )
            .add_layer(GeomLayer::point())
            .add_layer(GeomLayer::smooth_with_fit(LinearFit {
                slope: -0.04,
                intercept: 28.0,
            }));

        assert_eq!(spec.layers.len(), 2);
        assert!(spec.smooth_fit().is_some());
    }

    #[test]
    fn test_terminal_render_caps() {
        let spec = PlotSpec::new()
            .with_title("Test Scatter")
            .with_xy_data(vec![1.0, 2.0, 3.0], vec![10.0, 20.0, 30.0]);

        let caps = ghl_diagnostics::RenderCaps::rich_terminal(80);
        let card = spec.render(&caps);
        assert!(card.contains("Test Scatter"));
        assert!(card.contains("/ᐠ˵- ⩊ -˵マ ✧ READY"));
    }

    #[test]
    fn test_vega_json_generation() {
        let spec = PlotSpec::new()
            .with_title("Vega Test")
            .with_xy_data(vec![1.0, 2.0], vec![3.0, 4.0])
            .add_layer(GeomLayer::point());

        let json = spec.to_vega_json().unwrap();
        assert!(json.contains("https://vega.github.io/schema/vega-lite/v5.json"));
        assert!(json.contains("Vega Test"));
    }
}
