//! Code generation backend abstractions (Cranelift JIT & LLVM AOT).

pub struct CodegenEngine {
    pub target: String,
}

impl CodegenEngine {
    pub fn new(target: impl Into<String>) -> Self {
        Self { target: target.into() }
    }
}

