//! High-level and Mid-level Intermediate Representation for GHL.

pub struct HirModule {
    pub name: String,
}

impl HirModule {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}
