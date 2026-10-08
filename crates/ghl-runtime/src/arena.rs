//! Regional Memory Arenas powered by `bumpalo` (RFC 03 §2.2).
//!
//! Provides ultra-fast bump allocation inside iterative algorithms (MCMC, Bootstrap,
//! Gibbs Sampler) with $O(1)$ instant memory reclamation at iteration scope boundaries.

use crate::value::Value;
use crate::vector_data::VectorData;
use bumpalo::Bump;
use std::sync::Arc;

#[derive(Debug)]
pub struct ArenaState {
    bump: Bump,
    used_bytes: usize,
}

impl ArenaState {
    pub fn new() -> Self {
        Self {
            bump: Bump::new(),
            used_bytes: 0,
        }
    }

    /// O(1) instant memory reclamation at loop boundary.
    pub fn reset(&mut self) {
        self.bump.reset();
        self.used_bytes = 0;
    }

    /// Current live memory in bytes allocated by the arena.
    pub fn allocated_bytes(&self) -> usize {
        self.used_bytes
    }

    /// Total capacity in bytes currently held in chunks by the bump allocator.
    pub fn capacity_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }

    /// Allocates a vector inside the regional arena.
    pub fn alloc_vector(&mut self, len: usize, default: f64) -> Value {
        self.used_bytes += len * std::mem::size_of::<f64>();
        let slice = self.bump.alloc_slice_fill_copy(len, default);
        Value::Vector(VectorData::from_f64(slice.to_vec()))
    }

    /// Allocates a matrix inside the regional arena with CoW backing.
    pub fn alloc_matrix(&mut self, rows: usize, cols: usize, default: f64) -> Value {
        let len = rows * cols;
        self.used_bytes += len * std::mem::size_of::<f64>();
        let slice = self.bump.alloc_slice_fill_copy(len, default);
        Value::Matrix {
            rows,
            cols,
            data: Arc::new(slice.to_vec()),
        }
    }
}

impl Default for ArenaState {
    fn default() -> Self {
        Self::new()
    }
}
