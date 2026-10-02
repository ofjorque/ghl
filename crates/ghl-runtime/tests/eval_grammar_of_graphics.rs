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
