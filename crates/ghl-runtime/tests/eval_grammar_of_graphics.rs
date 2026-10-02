use ghl_runtime::{Interpreter, Value};
use ghl_syntax::parse;

fn eval_source(src: &str) -> Result<Value, String> {
    let program = parse(src).map_err(|e| format!("{:?}", e))?;
    let mut interp = Interpreter::new();
    interp.eval_program(&program).map_err(|e| format!("{:?}", e))
}

#[test]
fn test_grammar_of_graphics_plus_operator_layers() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0],
            y: [2.0, 4.1, 5.9, 8.2, 10.1]
        };
        let p = plot(df, aes("x", "y")) + geom_point() + geom_smooth() + theme_dark();
        p
    "#;
    let res = eval_source(src).expect("Plot evaluation should succeed");
    match res {
        Value::Plot(p) => {
            assert_eq!(p.layers.len(), 2, "Plot should have 2 layers (point + smooth)");
            assert!(p.smooth_fit().is_some(), "Smooth fit should be automatically computed");
            let fit = p.smooth_fit().unwrap();
            assert!((fit.slope - 2.0).abs() < 0.2, "Slope should be approximately 2.0");
            assert_eq!(p.theme, ghl_plot::PlotTheme::Dark);
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_grammar_of_graphics_pipe_syntax() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0],
            y: [10.0, 20.0, 30.0, 40.0]
        };
        let p = df 
            |> plot(aes("x", "y")) 
            |> geom_point() 
            |> geom_line() 
            |> theme_minimal();
        p
    "#;
    let res = eval_source(src).expect("Pipe plot evaluation should succeed");
    match res {
        Value::Plot(p) => {
            assert_eq!(p.layers.len(), 2, "Plot should have 2 layers (point + line)");
            assert_eq!(p.theme, ghl_plot::PlotTheme::Minimal);
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_grammar_of_graphics_color_grouping() {
    let src = r#"
        let df = dataframe {
            wt: [2.5, 2.8, 3.2, 3.5, 4.0, 4.2],
            mpg: [28.0, 26.0, 22.0, 20.0, 16.0, 15.0],
            cyl: ["4cyl", "4cyl", "6cyl", "6cyl", "8cyl", "8cyl"]
        };
        let p = ggplot(df, aes("wt", "mpg", "cyl")) + geom_point();
        p
    "#;
    let res = eval_source(src).expect("Color grouping plot should succeed");
    match res {
        Value::Plot(p) => {
            assert_eq!(p.series.len(), 3, "Should create 3 series for 4cyl, 6cyl, 8cyl");
            assert_eq!(p.labels.color_label.as_deref(), Some("cyl"));
            let grp_names: Vec<_> = p.series.iter().filter_map(|s| s.group_name.as_deref()).collect();
            assert!(grp_names.contains(&"4cyl"));
            assert!(grp_names.contains(&"6cyl"));
            assert!(grp_names.contains(&"8cyl"));
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_grammar_of_graphics_vega_export() {
    let src = r#"
        let df = dataframe {
            x: [10.0, 100.0, 1000.0],
            y: [1.0, 2.0, 3.0]
        };
        let p = plot(df, aes("x", "y")) + geom_point();
        let p_log = scale_x_log10(p);
        let json = to_vega_json(p_log);
        json
    "#;
    let res = eval_source(src).expect("Vega JSON export should succeed");
    match res {
        Value::String(s) => {
            assert!(s.contains("https://vega.github.io/schema/vega-lite/v5.json"));
            assert!(s.contains("\"type\": \"log\""), "Should contain log scale in Vega encoding");
        }
        other => panic!("Expected Value::String, got {:?}", other),
    }
}

#[test]
fn test_grammar_of_graphics_svg_export() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0],
            y: [4.0, 5.0, 6.0]
        };
        let p = plot(df, aes("x", "y")) + geom_point();
        let svg = to_svg(p, 400, 300);
        svg
    "#;
    let res = eval_source(src).expect("SVG export should succeed");
    match res {
        Value::String(s) => {
            assert!(s.starts_with("<svg") || s.contains("<svg"), "Should output valid SVG tag");
            assert!(s.ends_with("</svg>") || s.contains("</svg>"), "Should close SVG tag");
        }
        other => panic!("Expected Value::String, got {:?}", other),
    }
}

#[test]
fn test_grammar_of_graphics_histogram_and_boxplot() {
    let src = r#"
        let df = dataframe {
            vals: [10.0, 12.0, 14.0, 15.0, 18.0, 22.0, 25.0, 29.0, 35.0, 42.0]
        };
        let p_hist = plot(df, aes("vals")) + geom_histogram(5);
        let p_box = plot(df, aes("vals")) + geom_boxplot();
        [p_hist, p_box]
    "#;
    let res = eval_source(src).expect("Hist and Boxplot should succeed");
    match res {
        Value::Vector(v) => {
            assert_eq!(v.len(), 2);
            if let Value::Plot(ref h) = v[0] {
                assert!(h.layers.iter().any(|l| matches!(l.kind, ghl_plot::GeomKind::Histogram { bins: 5 })));
            } else {
                panic!("Expected plot");
            }
            if let Value::Plot(ref b) = v[1] {
                assert!(b.boxplot_stats().is_some(), "Boxplot stats should be automatically computed");
            } else {
                panic!("Expected plot");
            }
        }
        other => panic!("Expected Value::Vector, got {:?}", other),
    }
}

#[test]
fn test_grammar_of_graphics_comparative_boxplot() {
    let src = r#"
        let df = dataframe {
            species: ["Adelie", "Adelie", "Adelie", "Adelie", "Gentoo", "Gentoo", "Gentoo", "Gentoo"],
            mass: [3300.0, 3500.0, 3700.0, 3900.0, 4800.0, 5000.0, 5200.0, 5500.0]
        };
        let p = ggplot(df, aes("species", "mass")) + geom_boxplot();
        p
    "#;
    let res = eval_source(src).expect("Comparative boxplot should succeed");
    match res {
        Value::Plot(p) => {
            let multi_stats = p.boxplot_multi_stats();
            assert_eq!(multi_stats.len(), 2, "Should have stats for Adelie and Gentoo");
            assert_eq!(multi_stats[0].0, "Adelie");
            assert_eq!(multi_stats[1].0, "Gentoo");
            assert_eq!(multi_stats[0].1.median, 3700.0);
            assert_eq!(multi_stats[1].1.median, 5200.0);

            // Test Vega-Lite JSON export for comparative boxplot
            let json = p.to_vega_json().unwrap();
            assert!(json.contains("\"field\": \"category\""));
            assert!(json.contains("\"type\": \"boxplot\""));

            // Test SVG rendering for comparative boxplot
            let svg = p.to_svg(600, 400).unwrap();
            assert!(svg.contains("<svg"));

            // Test terminal card rendering
            let caps = ghl_diagnostics::RenderCaps::rich_terminal(80);
            let term = p.render(&caps);
            assert!(term.contains("Adelie"));
            assert!(term.contains("Gentoo"));
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_grammar_of_graphics_facet_wrap_composition() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 1.0, 2.0, 3.0],
            y: [2.0, 4.0, 6.0, 10.0, 20.0, 30.0],
            species: ["Adelie", "Adelie", "Adelie", "Chinstrap", "Chinstrap", "Chinstrap"]
        };
        let p = ggplot(df, aes("x", "y")) + geom_point() + geom_smooth() + facet_wrap("species", ncol = 2);
        p
    "#;
    let res = eval_source(src).expect("Facet wrap evaluation should succeed");
    match res {
        Value::Plot(p) => {
            assert!(p.facet.is_some(), "Plot should contain FacetSpec");
            let (panels, rows, cols) = p.partition_facets();
            assert_eq!(panels.len(), 2, "Should create 2 panels for Adelie and Chinstrap");
            assert_eq!(cols, 2, "ncol requested was 2");
            assert_eq!(rows, 1);

            // First panel: Adelie (slope ~ 2.0)
            let fit0 = panels[0].spec.smooth_fit().expect("Adelie panel must have local smooth fit");
            assert!((fit0.slope - 2.0).abs() < 1e-4, "Adelie slope should be 2.0, got {}", fit0.slope);

            // Second panel: Chinstrap (slope ~ 10.0)
            let fit1 = panels[1].spec.smooth_fit().expect("Chinstrap panel must have local smooth fit");
            assert!((fit1.slope - 10.0).abs() < 1e-4, "Chinstrap slope should be 10.0, got {}", fit1.slope);

            // Native SVG rendering
            let svg = p.to_svg(800, 600).expect("Faceted SVG should render");
            assert!(svg.contains("<svg"), "Must produce valid SVG XML");
            assert!(svg.contains("Adelie"), "SVG must include Adelie strip title");
            assert!(svg.contains("Chinstrap"), "SVG must include Chinstrap strip title");

            // Terminal ASCII rendering
            let caps = ghl_diagnostics::RenderCaps::rich_terminal(80);
            let term = p.render(&caps);
            assert!(term.contains("[ Adelie ]"), "Terminal output must render Adelie facet card");
            assert!(term.contains("[ Chinstrap ]"), "Terminal output must render Chinstrap facet card");

            // Vega-Lite JSON export
            let vega = p.to_vega_json().expect("Faceted Vega-Lite JSON export should succeed");
            assert!(vega.contains("\"facet\""), "Vega output must declare facet");
            assert!(vega.contains("\"field\": \"species\""), "Vega output must facet on species");
            assert!(vega.contains("\"columns\": 2"), "Vega output must specify 2 columns");
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_grammar_of_graphics_facet_wrap_formula_and_free_scales() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 100.0, 200.0],
            y: [5.0, 6.0, 500.0, 600.0],
            island: ["Biscoe", "Biscoe", "Dream", "Dream"]
        };
        let p = ggplot(df, aes("x", "y")) + geom_point() + facet_wrap("island", scales = "free");
        p
    "#;
    let res = eval_source(src).expect("Facet wrap with formula and free scales should succeed");
    match res {
        Value::Plot(p) => {
            let (panels, _, _) = p.partition_facets();
            assert_eq!(panels.len(), 2);
            assert_eq!(panels[0].label, "Biscoe");
            assert_eq!(panels[1].label, "Dream");

            // Free scales mean each panel has no fixed outer limits forced upon it
            assert!(panels[0].spec.x_limits.is_none());
            assert!(panels[0].spec.y_limits.is_none());

            let vega = p.to_vega_json().expect("Vega-Lite should serialize");
            assert!(vega.contains("\"resolve\""), "Free scales must specify resolve in Vega");
            assert!(vega.contains("\"independent\""), "Free scales must be independent in Vega");
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_grammar_of_graphics_facet_grid_composition() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0],
            y: [10.0, 20.0, 30.0, 40.0],
            drv: ["4", "4", "f", "f"],
            cyl: ["4cyl", "6cyl", "4cyl", "6cyl"]
        };
        let p = ggplot(df, aes("x", "y")) + geom_point() + facet_grid(drv ~ cyl);
        p
    "#;
    let res = eval_source(src).expect("Facet grid evaluation should succeed");
    match res {
        Value::Plot(p) => {
            assert!(p.facet.is_some());
            let (panels, rows, cols) = p.partition_facets();
            assert_eq!(rows, 2, "drv has 2 unique levels: 4, f");
            assert_eq!(cols, 2, "cyl has 2 unique levels: 4cyl, 6cyl");
            assert_eq!(panels.len(), 4, "2x2 grid produces 4 cross-product cells");

            let svg = p.to_svg(800, 600).expect("Facet grid SVG should render");
            assert!(svg.contains("<svg"));

            let vega = p.to_vega_json().expect("Facet grid Vega should render");
            assert!(vega.contains("\"row\""));
            assert!(vega.contains("\"column\""));
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_grammar_of_graphics_facet_pipe_syntax() {
    let src = r#"
        let df = dataframe {
            score: [85.0, 90.0, 78.0, 82.0],
            time: [10.0, 12.0, 8.0, 9.0],
            cohort: ["A", "A", "B", "B"]
        };
        let p = df 
            |> ggplot(aes("time", "score")) 
            |> geom_point() 
            |> facet_wrap("cohort", ncol = 1);
        p
    "#;
    let res = eval_source(src).expect("Pipe facet wrap should succeed");
    match res {
        Value::Plot(p) => {
            assert!(p.facet.is_some());
            let (panels, rows, cols) = p.partition_facets();
            assert_eq!(panels.len(), 2);
            assert_eq!(cols, 1);
            assert_eq!(rows, 2);
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_patchwork_horizontal_operator() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0],
            y: [10.0, 20.0, 30.0]
        };
        let p1 = ggplot(df, aes("x", "y")) + geom_point() + labs(title = "Plot 1");
        let p2 = ggplot(df, aes("x", "y")) + geom_line() + labs(title = "Plot 2");
        let comp = p1 | p2;
        comp
    "#;
    let res = eval_source(src).expect("Patchwork horizontal composition should succeed");
    match res {
        Value::Plot(p) => {
            assert!(p.composite.is_some(), "Plot must contain a composite layout");
            match p.composite.as_deref().unwrap() {
                ghl_plot::CompositePlot::Horizontal(left, right) => {
                    assert_eq!(left.labels.title.as_deref(), Some("Plot 1"));
                    assert_eq!(right.labels.title.as_deref(), Some("Plot 2"));
                }
                _ => panic!("Expected Horizontal composite"),
            }

            // Test multi-backend rendering
            let svg = p.to_svg(800, 400).expect("Composite SVG should render");
            assert!(svg.contains("<svg"), "SVG must have root element");

            let vega = p.to_vega_json().expect("Composite Vega-Lite should render");
            assert!(vega.contains("\"hconcat\""), "Vega must contain hconcat operator");

            let caps = ghl_diagnostics::RenderCaps::ascii_plain(80);
            let deck = p.render(&caps);
            assert!(!deck.is_empty(), "Terminal deck card should render");
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_patchwork_vertical_operator() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0],
            y: [5.0, 15.0, 25.0]
        };
        let p1 = ggplot(df, aes("x", "y")) + geom_point();
        let p2 = ggplot(df, aes("x", "y")) + geom_bar();
        let comp = p1 / p2;
        comp
    "#;
    let res = eval_source(src).expect("Patchwork vertical composition should succeed");
    match res {
        Value::Plot(p) => {
            assert!(p.composite.is_some());
            match p.composite.as_deref().unwrap() {
                ghl_plot::CompositePlot::Vertical(_, _) => {}
                _ => panic!("Expected Vertical composite"),
            }

            let svg = p.to_svg(600, 800).expect("Vertical composite SVG should render");
            assert!(svg.contains("<svg"));

            let vega = p.to_vega_json().expect("Vertical composite Vega should render");
            assert!(vega.contains("\"vconcat\""), "Vega must contain vconcat operator");
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_patchwork_nested_layout_and_precedence() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0],
            y: [10.0, 20.0, 30.0]
        };
        let p1 = ggplot(df, aes("x", "y")) + geom_point() + labs(title = "P1");
        let p2 = ggplot(df, aes("x", "y")) + geom_line() + labs(title = "P2");
        let p3 = ggplot(df, aes("x", "y")) + geom_smooth() + labs(title = "P3");

        // Division / binds tighter than |: p1 | p2 / p3 is p1 | (p2 / p3)
        let c1 = p1 | p2 / p3;

        // Parentheses allow (p1 | p2) / p3
        let c2 = (p1 | p2) / p3;

        c2
    "#;
    let res = eval_source(src).expect("Nested patchwork evaluation should succeed");
    match res {
        Value::Plot(p) => {
            assert!(p.composite.is_some());
            match p.composite.as_deref().unwrap() {
                ghl_plot::CompositePlot::Vertical(top, bottom) => {
                    assert!(top.composite.is_some(), "Top should be composite horizontal");
                    assert_eq!(bottom.labels.title.as_deref(), Some("P3"));
                }
                _ => panic!("Expected outer Vertical composite"),
            }

            let svg = p.to_svg(1000, 800).expect("Nested SVG should render");
            assert!(svg.contains("<svg"));
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_patchwork_pipeline_verbs() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0],
            y: [10.0, 20.0]
        };
        let p1 = ggplot(df, aes("x", "y")) + geom_point();
        let p2 = ggplot(df, aes("x", "y")) + geom_line();

        let comp1 = beside(p1, p2);
        let comp2 = p1 |> stack(p2);

        comp2
    "#;
    let res = eval_source(src).expect("Patchwork verbs should succeed");
    match res {
        Value::Plot(p) => {
            assert!(p.composite.is_some());
            match p.composite.as_deref().unwrap() {
                ghl_plot::CompositePlot::Vertical(_, _) => {}
                _ => panic!("Expected Vertical composite"),
            }
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_plot_theme_and_system_font() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0],
            y: [10.0, 20.0, 30.0]
        };
        let p = ggplot(df, aes("x", "y")) 
            + geom_point() 
            + labs(title = "Custom System Font")
            + theme(font = "Fira Code", style = "minimal");
        p
    "#;
    let res = eval_source(src).expect("Theme with system font should succeed");
    match res {
        Value::Plot(p) => {
            assert_eq!(p.font_family.as_deref(), Some("Fira Code"));
            assert_eq!(p.theme, ghl_plot::PlotTheme::Minimal);

            // Verify SVG embeds the system font family name
            let svg = p.to_svg(800, 600).expect("SVG with font should render");
            assert!(svg.contains("Fira Code"), "SVG output must include the custom font family");

            // Verify Vega-Lite specification configures the font
            let vega = p.to_vega_json().expect("Vega with font should render");
            assert!(vega.contains("\"font\": \"Fira Code\""), "Vega config must include font");
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}

#[test]
fn test_patchwork_inherited_font_on_composite() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0],
            y: [5.0, 10.0]
        };
        let p1 = ggplot(df, aes("x", "y")) + geom_point();
        let p2 = ggplot(df, aes("x", "y")) + geom_line();
        let comp = (p1 | p2) + theme(font = "Inter");
        comp
    "#;
    let res = eval_source(src).expect("Composite theme with font should succeed");
    match res {
        Value::Plot(p) => {
            assert_eq!(p.font_family.as_deref(), Some("Inter"));

            let svg = p.to_svg(800, 400).expect("Composite SVG with font should render");
            assert!(svg.contains("Inter"), "Composite SVG must propagate inherited font");

            let vega = p.to_vega_json().expect("Composite Vega with font should render");
            assert!(vega.contains("\"font\": \"Inter\""), "Composite Vega must contain font config");
        }
        other => panic!("Expected Value::Plot, got {:?}", other),
    }
}
