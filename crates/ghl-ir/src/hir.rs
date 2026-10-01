//! High-Level Intermediate Representation (HIR) for GHL.
//!
//! Provides typed, lowered representations of GHL functions and expressions
//! ready for JIT compilation via Cranelift or AOT codegen.

use std::collections::HashMap;
use ghl_diagnostics::Diagnostic;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirType {
    I64,
    F64,
    Bool,
    Unit,
}

impl HirType {
    pub fn is_float(&self) -> bool {
        matches!(self, Self::F64)
    }

    pub fn is_int(&self) -> bool {
        matches!(self, Self::I64)
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, Self::Bool)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,

    // Relational
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Logical
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirLiteral {
    I64(i64),
    F64(f64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirExpr {
    Literal(HirLiteral, HirType),
    Var(String, HirType),
    Binary {
        op: HirBinaryOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
        ty: HirType,
    },
    Unary {
        op: HirUnaryOp,
        operand: Box<HirExpr>,
        ty: HirType,
    },
    Call {
        func: String,
        args: Vec<HirExpr>,
        ty: HirType,
    },
    IfElse {
        cond: Box<HirExpr>,
        then_branch: Box<HirExpr>,
        else_branch: Box<HirExpr>,
        ty: HirType,
    },
    Block {
        statements: Vec<HirStatement>,
        result: Option<Box<HirExpr>>,
        ty: HirType,
    },
    While {
        cond: Box<HirExpr>,
        body: Box<HirExpr>,
        ty: HirType,
    },
    For {
        var: String,
        start: Box<HirExpr>,
        end: Box<HirExpr>,
        body: Box<HirExpr>,
        ty: HirType,
    },
}

impl HirExpr {
    pub fn ty(&self) -> HirType {
        match self {
            Self::Literal(_, ty) => *ty,
            Self::Var(_, ty) => *ty,
            Self::Binary { ty, .. } => *ty,
            Self::Unary { ty, .. } => *ty,
            Self::Call { ty, .. } => *ty,
            Self::IfElse { ty, .. } => *ty,
            Self::Block { ty, .. } => *ty,
            Self::While { ty, .. } => *ty,
            Self::For { ty, .. } => *ty,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirStatement {
    Let {
        name: String,
        ty: HirType,
        value: HirExpr,
    },
    Assign {
        name: String,
        value: HirExpr,
    },
    Expr(HirExpr),
    Return(Option<HirExpr>),
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirParam {
    pub name: String,
    pub ty: HirType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<HirParam>,
    pub return_ty: HirType,
    pub body: HirExpr,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HirModule {
    pub name: String,
    pub functions: HashMap<String, HirFunction>,
    /// Named functions whose body could not be lowered for scalar JIT/AOT
    /// (e.g. they use types or expressions outside the supported subset),
    /// paired with why. `ghl run` silently falls back to the interpreter for
    /// these; `ghl build` treats a non-empty list as a hard error naming
    /// each function, since an AOT binary can't silently drop a function.
    pub skipped: Vec<(String, Diagnostic)>,
}

impl HirModule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            functions: HashMap::new(),
            skipped: Vec::new(),
        }
    }

    pub fn add_function(&mut self, func: HirFunction) {
        self.functions.insert(func.name.clone(), func);
    }
}

