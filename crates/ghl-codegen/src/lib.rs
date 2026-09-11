//! Code generation and Cranelift JIT engine for GHL.

pub mod compiler;
pub mod jit;

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
}

