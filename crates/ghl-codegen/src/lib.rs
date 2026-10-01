//! Code generation and Cranelift JIT engine for GHL.

pub mod aot;
pub mod compiler;
pub mod host;
pub mod jit;

pub use aot::AotEngine;
pub use compiler::FunctionCompiler;
pub use jit::JitEngine;

#[cfg(test)]
mod tests {
    use super::*;
    use ghl_ir::lower_ast;
    use ghl_syntax::parse;

    #[test]
    fn test_jit_compile_and_execute_arithmetic() {
        let code = r#"
            fn add_nums(a, b) {
                let x = a * 2;
                let y = b + 10;
                x + y
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let add_fn = jit.get_fn_i64_2("add_nums").expect("add_nums compiled");
        // a = 5, b = 7 -> (5 * 2) + (7 + 10) = 10 + 17 = 27
        let result = add_fn(5, 7);
        assert_eq!(result, 27);
    }

    #[test]
    fn test_jit_compile_and_execute_fibonacci() {
        let code = r#"
            fn fib(n) {
                if n <= 1 {
                    n
                } else {
                    fib(n - 1) + fib(n - 2)
                }
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let fib_fn = jit.get_fn_i64_1("fib").expect("fib compiled");
        // fib(0) = 0, fib(1) = 1, fib(2) = 1, fib(3) = 2, fib(4) = 3, fib(5) = 5, fib(10) = 55
        assert_eq!(fib_fn(0), 0);
        assert_eq!(fib_fn(1), 1);
        assert_eq!(fib_fn(5), 5);
        assert_eq!(fib_fn(10), 55);
        assert_eq!(fib_fn(15), 610);
    }

    #[test]
    fn test_jit_compile_float_operations() {
        let code = r#"
            fn hypot_approx(a: f64, b: f64) -> f64 {
                let a2 = a * a;
                let b2 = b * b;
                a2 + b2
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let hypot_fn = jit.get_fn_f64_2("hypot_approx").expect("hypot_approx compiled");
        // 3.0^2 + 4.0^2 = 9.0 + 16.0 = 25.0
        let res = hypot_fn(3.0, 4.0);
        assert!((res - 25.0).abs() < 1e-6);
    }

    #[test]
    fn test_jit_compile_early_return() {
        let code = r#"
            fn sign(n: i64) -> i64 {
                if n > 0 {
                    return 1;
                };
                if n < 0 {
                    return -1;
                };
                return 0;
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let sign_fn = jit.get_fn_i64_1("sign").expect("sign compiled");
        assert_eq!(sign_fn(42), 1);
        assert_eq!(sign_fn(-10), -1);
        assert_eq!(sign_fn(0), 0);
    }

    #[test]
    fn test_jit_compile_both_branches_return() {
        let code = r#"
            fn abs_val(n: i64) -> i64 {
                if n < 0 {
                    return -n;
                } else {
                    return n;
                }
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let abs_fn = jit.get_fn_i64_1("abs_val").expect("abs_val compiled");
        assert_eq!(abs_fn(-5), 5);
        assert_eq!(abs_fn(7), 7);
    }

    #[test]
    fn test_jit_compile_dead_code_after_return() {
        let code = r#"
            fn dead_branch(n: i64) -> i64 {
                return n * 2;
                let unused = n + 100;
                return unused;
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let dead_fn = jit.get_fn_i64_1("dead_branch").expect("dead_branch compiled");
        assert_eq!(dead_fn(21), 42);
    }

    #[test]
    fn test_aot_compile_object_file() {
        let code = r#"
            fn multiply(a: i64, b: i64) -> i64 {
                a * b
            }

            fn square(x: i64) -> i64 {
                multiply(x, x)
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let aot = AotEngine::new("test_module").expect("aot init ok");
        let obj_bytes = aot.compile_module(&hir_module).expect("aot compilation ok");

        assert!(!obj_bytes.is_empty(), "Object bytes should not be empty");
        // Verify that the bytes are a valid object file format parseable by `object`
        let obj_file = cranelift_object::object::File::parse(&*obj_bytes).expect("parse valid object file");
        use cranelift_object::object::{Object, ObjectSymbol};
        let symbols: Vec<_> = obj_file.symbols().filter_map(|s| s.name().ok()).collect();
        assert!(symbols.contains(&"multiply"), "Emitted object should contain exported `multiply` symbol");
        assert!(symbols.contains(&"square"), "Emitted object should contain exported `square` symbol");
    }

    #[test]
    fn test_top_level_script_jit_and_aot() {
        let code = r#"
            let a = 15;
            let b = 27;
            a + b;
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        // Test JIT execution
        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");
        let main_fn = jit.get_fn_i64_0("__ghl_main").expect("__ghl_main exists");
        assert_eq!(main_fn(), 42);

        // Test AOT object emission
        let aot = AotEngine::new("script_main").expect("aot init ok");
        let obj_bytes = aot.compile_module(&hir_module).expect("aot compilation ok");
        let obj_file = cranelift_object::object::File::parse(&*obj_bytes).expect("parse valid object file");
        use cranelift_object::object::{Object, ObjectSymbol};
        let symbols: Vec<_> = obj_file.symbols().filter_map(|s| s.name().ok()).collect();
        assert!(symbols.contains(&"__ghl_main"), "Object should export `__ghl_main`");
    }

    #[test]
    fn test_jit_tail_recursion_100k() {
        let code = r#"
            fn sum_tail_rec(n: i64, acc: i64) -> i64 {
                if n <= 0 {
                    acc
                } else {
                    sum_tail_rec(n - 1, acc + n)
                }
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let sum_fn = jit.get_fn_i64_2("sum_tail_rec").expect("compiled sum_tail_rec");

        let t0 = std::time::Instant::now();
        let res = sum_fn(100_000, 0);
        let elapsed = t0.elapsed();

        assert_eq!(res, 5000050000, "100k tail-recursive sum must match Gauss formula");
        assert!(elapsed.as_millis() < 50, "Native TCO must run 100k iterations in milliseconds (took {:?})", elapsed);
    }

    #[test]
    fn test_jit_host_math_libcall() {
        let code = r#"
            fn calc_hypot(a: f64, b: f64) -> f64 {
                let sum_sq = a * a + b * b;
                sqrt(sum_sq)
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let hypot_fn = jit.get_fn_f64_2("calc_hypot").expect("compiled calc_hypot");
        let res = hypot_fn(3.0, 4.0);
        assert!((res - 5.0).abs() < 1e-6, "sqrt(3^2 + 4^2) must equal 5.0");
    }

    #[test]
    fn test_jit_compile_while_loop() {
        let code = r#"
            fn sum_while(n: i64) -> i64 {
                let mut total = 0;
                let mut i = 1;
                while i <= n {
                    total = total + i;
                    i = i + 1;
                }
                total
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let sum_fn = jit.get_fn_i64_1("sum_while").expect("compiled sum_while");
        assert_eq!(sum_fn(10), 55);
        assert_eq!(sum_fn(100), 5050);

        let t0 = std::time::Instant::now();
        let res = sum_fn(1_000_000);
        let elapsed = t0.elapsed();

        assert_eq!(res, 500000500000);
        assert!(elapsed.as_millis() < 50, "1M iteration while-loop in native Cranelift must run in <50ms (took {:?})", elapsed);
    }

    #[test]
    fn test_jit_compile_for_loop() {
        let code = r#"
            fn sum_for(n: i64) -> i64 {
                let mut total = 0;
                for i in 1..n + 1 {
                    total = total + i;
                }
                total
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let sum_fn = jit.get_fn_i64_1("sum_for").expect("compiled sum_for");
        assert_eq!(sum_fn(10), 55);
        assert_eq!(sum_fn(100), 5050);
        assert_eq!(sum_fn(1000), 500500);
    }

    #[test]
    fn test_jit_compile_loop_with_break_and_continue() {
        let code = r#"
            fn sum_evens_until(limit: i64) -> i64 {
                let mut total = 0;
                let mut i = 0;
                while i < 1000 {
                    i = i + 1;
                    if i % 2 != 0 {
                        continue;
                    }
                    if total + i > limit {
                        break;
                    }
                    total = total + i;
                }
                total
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let sum_fn = jit.get_fn_i64_1("sum_evens_until").expect("compiled sum_evens_until");
        // evens: 2, 4, 6, 8, 10
        // cumsums: 2, 6, 12, 20, 30
        assert_eq!(sum_fn(15), 12);
        assert_eq!(sum_fn(25), 20);
        assert_eq!(sum_fn(35), 30);
    }

    #[test]
    fn test_jit_numerical_floating_point_solver() {
        let code = r#"
            fn newton_sqrt(x: f64) -> f64 {
                let mut guess = x / 2.0;
                let mut iter = 0;
                while iter < 25 {
                    guess = 0.5 * (guess + x / guess);
                    iter = iter + 1;
                }
                guess
            }
        "#;
        let program = parse(code).expect("syntax ok");
        let hir_module = lower_ast(&program).expect("hir ok");

        let mut jit = JitEngine::new().expect("jit init ok");
        jit.compile_module(&hir_module).expect("jit compilation ok");

        let sqrt_fn = jit.get_fn_f64_1("newton_sqrt").expect("compiled newton_sqrt");
        assert!((sqrt_fn(2.0) - std::f64::consts::SQRT_2).abs() < 1e-10);
        assert!((sqrt_fn(144.0) - 12.0).abs() < 1e-10);
        assert!((sqrt_fn(625.0) - 25.0).abs() < 1e-10);
    }
}

