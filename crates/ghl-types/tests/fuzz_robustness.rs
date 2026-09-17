//! Fuzz and Robustness Test Suite for GHL Compiler (Roadmap 07).
//!
//! Enforces the zero-panic, zero-crash invariant: ensures that no malformed syntax,
//! unbalanced delimiters, deeply nested expressions, truncated files, or random byte
//! streams can panic the lexer, parser, or typechecker.

use ghl_syntax::parser::parse;
use ghl_types::check;

fn run_with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
    let handle = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(f)
        .unwrap();
    handle.join().unwrap();
}

#[test]
fn test_fuzz_unbalanced_delimiters() {
    run_with_large_stack(|| {
        let inputs = [
            "((((((((((",
            "))))))))))",
            "[[[[[[[[[[",
            "]]]]]]]]]]",
            "{{{{{{{{{{",
            "}}}}}}}}}}",
            "([{(",
            ")}]",
            "#[#[#[#[#[",
            "]#]#]#]#]#",
            "/* unclosed block comment",
            "/* nested /* unclosed comment */",
            "\"unclosed string literal",
            "\"unclosed string with escaped \\\" quote",
            "\"unclosed string with newline\n",
        ];

        for input in &inputs {
            let res = std::panic::catch_unwind(|| {
                let _ = parse(input);
            });
            assert!(res.is_ok(), "Parser must not panic on input: {:?}", input);
        }
    });
}

#[test]
fn test_fuzz_truncated_valid_programs() {
    let full_program = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0],
            y: [10.0, 20.0, 30.0]
        };

        fn compute(a: f64, b: f64) -> f64 {
            let s = a + b * 2.0;
            return s;
        }

        let model = ols(y ~ x, df);
        let betas = coef(model);
        println("Result:", betas);
    "#;

    // Truncate at every single character boundary from length 1 to full_program.len()
    for len in 1..full_program.len() {
        let truncated = &full_program[..len];
        let res = std::panic::catch_unwind(|| {
            if let Ok(prog) = parse(truncated) {
                let _ = check(&prog, "fuzz.gh", truncated);
            }
        });
        assert!(res.is_ok(), "Parser / typechecker panicked on truncation at byte {}", len);
    }
}

#[test]
fn test_fuzz_deeply_nested_structures() {
    run_with_large_stack(|| {
        // 50 levels of nested parentheses
        let mut deep_parens = String::new();
        for _ in 0..50 {
            deep_parens.push('(');
        }
        deep_parens.push_str("42");
        for _ in 0..50 {
            deep_parens.push(')');
        }

        let res = std::panic::catch_unwind(|| {
            let _ = parse(&deep_parens);
        });
        assert!(res.is_ok(), "Deeply nested parentheses must not panic");

        // 50 levels of chained pipes
        let mut deep_pipes = "1.0".to_string();
        for _ in 0..50 {
            deep_pipes.push_str(" |> abs()");
        }

        let res = std::panic::catch_unwind(|| {
            let _ = parse(&deep_pipes);
        });
        assert!(res.is_ok(), "Deeply chained pipelines must not panic");
    });
}

#[test]
fn test_fuzz_malformed_formulas_and_literals() {
    let inputs = [
        "~",
        "y ~",
        "~ x",
        "y ~ ~ x",
        "y ~ + + + x",
        "dataframe {",
        "dataframe { a: }",
        "dataframe { ,,, }",
        "mat [",
        "mat [ 1.0, ; ]",
        "mat [ ; ; ; ]",
        "let mut = 10;",
        "let = 10;",
        "let x: = 10;",
        "fn () -> {}",
        "fn test( { }",
        "export ffi",
        "match x { }",
        "match { => }",
        "for in 1..10 {}",
        "while { }",
        "if else",
        "NA:",
        "NA:1234 invalid",
    ];

    for input in &inputs {
        let res = std::panic::catch_unwind(|| {
            if let Ok(prog) = parse(input) {
                let _ = check(&prog, "fuzz.gh", input);
            }
        });
        assert!(res.is_ok(), "Compiler must handle malformed syntax cleanly without panic: {:?}", input);
    }
}

#[test]
fn test_fuzz_pseudorandom_byte_mutations() {
    // Deterministic pseudo-random seed
    let base = "let x = 10; fn foo(y: f64) -> f64 { y * 2.0 } let res = foo(x);";
    let mut rng: u64 = 0x12345678_abcdef01;

    for _ in 0..500 {
        // Linear congruential generator step
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut mutated = base.as_bytes().to_vec();

        let mutations = ((rng >> 32) % 15) as usize + 1;
        for _ in 0..mutations {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let pos = ((rng >> 32) as usize) % mutated.len();
            let byte_val = (rng & 0xFF) as u8;
            mutated[pos] = byte_val;
        }

        let input = String::from_utf8_lossy(&mutated);
        let res = std::panic::catch_unwind(|| {
            if let Ok(prog) = parse(&input) {
                let _ = check(&prog, "fuzz.gh", &input);
            }
        });
        assert!(res.is_ok(), "Fuzzer panic on random mutation: {:?}", input);
    }
}
