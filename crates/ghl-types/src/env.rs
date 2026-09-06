use std::collections::HashMap;
use crate::types::Type;

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolInfo {
    pub ty: Type,
    pub is_mut: bool,
}

#[derive(Debug, Clone)]
pub struct TypeEnv {
    scopes: Vec<HashMap<String, SymbolInfo>>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn with_prelude() -> Self {
        let mut env = Self::new();

        // Register Standard Library Statistics
        env.insert(
            "mean".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "median".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "sum".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "var".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "std_dev".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::F64),
            },
            false,
        );
        env.insert(
            "min".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "max".into(),
            Type::Function {
                params: vec![Type::Vector(Box::new(Type::Any))],
                ret: Box::new(Type::Any),
            },
            false,
        );

        // Register DataFrame verbs
        env.insert(
            "col".into(),
            Type::Function {
                params: vec![Type::String],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "filter".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "select".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "mutate".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "arrange".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "group_by".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "summarize".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );

        // Register Linear Algebra
        env.insert(
            "inv".into(),
            Type::Function {
                params: vec![Type::Matrix(Box::new(Type::F64))],
                ret: Box::new(Type::Matrix(Box::new(Type::F64))),
            },
            false,
        );
        env.insert(
            "transpose".into(),
            Type::Function {
                params: vec![Type::Matrix(Box::new(Type::Any))],
                ret: Box::new(Type::Matrix(Box::new(Type::Any))),
            },
            false,
        );
        env.insert(
            "det".into(),
            Type::Function {
                params: vec![Type::Matrix(Box::new(Type::F64))],
                ret: Box::new(Type::F64),
            },
            false,
        );

        // Mascot macros/helpers
        env.insert(
            "purr".into(),
            Type::Function {
                params: vec![Type::String],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "pounce".into(),
            Type::Function {
                params: vec![Type::Bool, Type::String],
                ret: Box::new(Type::Unit),
            },
            false,
        );

        // Print helpers
        env.insert(
            "println".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "print".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );

        // NEKO Statistical Modeling Verbs
        env.insert(
            "fit".into(),
            Type::Function {
                params: vec![Type::Formula, Type::Any],
                ret: Box::new(Type::ModelFit),
            },
            false,
        );
        env.insert(
            "ols".into(),
            Type::Function {
                params: vec![Type::Formula, Type::Any],
                ret: Box::new(Type::ModelFit),
            },
            false,
        );
        env.insert(
            "summary".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "tidy".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "glance".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "augment".into(),
            Type::Function {
                params: vec![Type::Any, Type::Any],
                ret: Box::new(Type::DataFrame(Vec::new())),
            },
            false,
        );
        env.insert(
            "residuals".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Vector(Box::new(Type::F64))),
            },
            false,
        );
        env.insert(
            "predict".into(),
            Type::Function {
                params: vec![Type::Any, Type::Any],
                ret: Box::new(Type::Vector(Box::new(Type::F64))),
            },
            false,
        );
        env.insert(
            "coef".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Vector(Box::new(Type::F64))),
            },
            false,
        );
        env.insert(
            "vcov".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Matrix(Box::new(Type::F64))),
            },
            false,
        );

        // Grammar of Graphics (RFC 09) Verbs
        env.insert(
            "plot".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "aes".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Any),
            },
            false,
        );
        env.insert(
            "geom_point".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "geom_line".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "geom_smooth".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "geom_histogram".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "geom_boxplot".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "geom_bar".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "labs".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "show".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Unit),
            },
            false,
        );
        env.insert(
            "scatter".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "hist".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "histogram".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );
        env.insert(
            "boxplot".into(),
            Type::Function {
                params: vec![Type::Any],
                ret: Box::new(Type::Plot),
            },
            false,
        );

        env
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn insert(&mut self, name: String, ty: Type, is_mut: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, SymbolInfo { ty, is_mut });
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&SymbolInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }
}
