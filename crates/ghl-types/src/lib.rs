//! Type system definitions, Factor types, and NA reasoning for GHL.
//!
//! Implements RFC 02 (Type System & NA semantics) and RFC 09 (Factors & Categorical Data).

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NAReason {
    NoResponse,
    NotObservable,
    NotApplicable,
    SensorDropout,
    BelowDetectionLimit,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum GhlValue<T> {
    Value(T),
    NA(Option<NAReason>),
}

impl<T> GhlValue<T> {
    pub fn is_na(&self) -> bool {
        matches!(self, Self::NA(_))
    }

    pub fn na_reason(&self) -> Option<&NAReason> {
        match self {
            Self::NA(Some(reason)) => Some(reason),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContrastScheme {
    Treatment,
    Sum,
    Helmert,
    Polynomial,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Factor {
    pub levels: Vec<String>,
    pub indices: Vec<usize>,
    pub contrast: ContrastScheme,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderedFactor {
    pub factor: Factor,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    I64,
    F64,
    Bool,
    String,
    Vector(Box<Type>),
    Matrix(Box<Type>),
    DataFrame,
    Factor,
    OrderedFactor,
    Formula,
    Void,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_na_with_optional_reason() {
        let plain_na: GhlValue<f64> = GhlValue::NA(None);
        let reasoned_na: GhlValue<f64> = GhlValue::NA(Some(NAReason::NoResponse));
        let observed: GhlValue<f64> = GhlValue::Value(42.0);

        assert!(plain_na.is_na());
        assert_eq!(plain_na.na_reason(), None);

        assert!(reasoned_na.is_na());
        assert_eq!(reasoned_na.na_reason(), Some(&NAReason::NoResponse));

        assert!(!observed.is_na());
        assert_eq!(observed.na_reason(), None);
    }
}
