//! Runtime engine for GHL: memory management, arenas, and statistical execution.

pub struct RuntimeContext {
    pub memory_limit_mb: usize,
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self {
            memory_limit_mb: 4096,
        }
    }
}

