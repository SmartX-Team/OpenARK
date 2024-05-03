use std::ops::{Add, Div, Mul, Neg, Not, Sub};

use anyhow::{bail, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ops::{And, Eq, Ge, Gt, Le, Lt, Ne, Or};

pub trait NetworkVirtualMachine {}

#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    pub code: Vec<Instruction>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Instruction {
    pub name: Option<String>,
    pub stmt: Stmt,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Stmt {
    Identity {
        index: usize,
    },
    DefineLocalFeature {
        value: Option<Feature>,
    },
    DefineLocalValue {
        value: Option<Number>,
    },
    BinaryExpr {
        lhs: Value,
        rhs: Value,
        op: BinaryExpr,
    },
    UnaryExpr {
        src: Value,
        op: UnaryExpr,
    },
}

impl From<Value> for Stmt {
    fn from(value: Value) -> Self {
        match value {
            Value::Feature(value) => Self::DefineLocalFeature { value: Some(value) },
            Value::Number(value) => Self::DefineLocalValue { value: Some(value) },
            Value::Variable(index) => Self::Identity { index },
        }
    }
}

impl From<Option<Feature>> for Stmt {
    fn from(value: Option<Feature>) -> Self {
        Self::DefineLocalFeature { value }
    }
}

impl From<Option<Number>> for Stmt {
    fn from(value: Option<Number>) -> Self {
        Self::DefineLocalValue { value }
    }
}

impl Stmt {
    pub const fn to_value(&self) -> Option<Value> {
        match self {
            Stmt::Identity { index } => Some(Value::Variable(*index)),
            Stmt::DefineLocalFeature { value: Some(value) } => Some(Value::Feature(*value)),
            Stmt::DefineLocalFeature { value: None } => None,
            Stmt::DefineLocalValue { value: Some(value) } => Some(Value::Number(*value)),
            Stmt::DefineLocalValue { value: None } => None,
            Stmt::BinaryExpr { .. } => None,
            Stmt::UnaryExpr { .. } => None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Value {
    Feature(Feature),
    Number(Number),
    Variable(usize),
}

macro_rules! impl_expr_unary {
    ( impl $name:ident ($fn:ident) for $src:ident as Feature -> Feature ) => {{
        match $src.to_feature()? {
            Some(src) => Ok(Stmt::DefineLocalFeature {
                value: Some(src.$fn()),
            }),
            _ => Ok(Stmt::UnaryExpr {
                src: $src,
                op: UnaryExpr::$name,
            }),
        }
    }};
    ( impl $name:ident ($fn:ident) for $src:ident as Number -> Number ) => {{
        match $src.to_number()? {
            Some(src) => Ok(Stmt::DefineLocalValue {
                value: Some(src.$fn()),
            }),
            _ => Ok(Stmt::UnaryExpr {
                src: $src,
                op: UnaryExpr::$name,
            }),
        }
    }};
}

impl Neg for Value {
    type Output = Result<Stmt>;

    fn neg(self) -> Self::Output {
        impl_expr_unary!(impl Neg(neg) for self as Number -> Number)
    }
}

impl Not for Value {
    type Output = Result<Stmt>;

    fn not(self) -> Self::Output {
        impl_expr_unary!(impl Not(not) for self as Feature -> Feature)
    }
}

macro_rules! impl_expr_binary {
    ( impl $ty:ident ($fn:ident) for Feature -> Feature ) => {
        impl $ty for Value {
            type Output = Result<Stmt>;

            fn $fn(self, rhs: Self) -> Self::Output {
                match (self.to_feature()?, rhs.to_feature()?) {
                    (Some(lhs), Some(rhs)) => Ok(Stmt::DefineLocalFeature {
                        value: Some(lhs.$fn(rhs)),
                    }),
                    (_, _) => Ok(Stmt::BinaryExpr {
                        lhs: self,
                        rhs,
                        op: BinaryExpr::$ty,
                    }),
                }
            }
        }
    };
    ( impl $ty:ident ($fn:ident) for Number -> Feature ) => {
        impl $ty for Value {
            type Output = Result<Stmt>;

            fn $fn(self, rhs: Self) -> Self::Output {
                match (self.to_number()?, rhs.to_number()?) {
                    (Some(lhs), Some(rhs)) => Ok(Stmt::DefineLocalFeature {
                        value: Some(lhs.$fn(rhs)),
                    }),
                    (_, _) => Ok(Stmt::BinaryExpr {
                        lhs: self,
                        rhs,
                        op: BinaryExpr::$ty,
                    }),
                }
            }
        }
    };
    ( impl $ty:ident ($fn:ident) for Number -> Number ) => {
        impl $ty for Value {
            type Output = Result<Stmt>;

            fn $fn(self, rhs: Self) -> Self::Output {
                match (self.to_number()?, rhs.to_number()?) {
                    (Some(lhs), Some(rhs)) => Ok(Stmt::DefineLocalValue {
                        value: Some(lhs.$fn(rhs)),
                    }),
                    (_, _) => Ok(Stmt::BinaryExpr {
                        lhs: self,
                        rhs,
                        op: BinaryExpr::$ty,
                    }),
                }
            }
        }
    };
    ( impl $ty:ident ($fn:ident) for Number -> Number? ) => {
        impl $ty for Value {
            type Output = Result<Stmt>;

            fn $fn(self, rhs: Self) -> Self::Output {
                match (self.to_number()?, rhs.to_number()?) {
                    (Some(lhs), Some(rhs)) => Ok(Stmt::DefineLocalValue {
                        value: Some(lhs.$fn(rhs)?),
                    }),
                    (_, _) => Ok(Stmt::BinaryExpr {
                        lhs: self,
                        rhs,
                        op: BinaryExpr::$ty,
                    }),
                }
            }
        }
    };
}

impl_expr_binary!(impl Add(add) for Number -> Number);
impl_expr_binary!(impl Sub(sub) for Number -> Number);
impl_expr_binary!(impl Mul(mul) for Number -> Number);
impl_expr_binary!(impl Div(div) for Number -> Number?);
impl_expr_binary!(impl Eq(eq) for Number -> Feature);
impl_expr_binary!(impl Ne(ne) for Number -> Feature);
impl_expr_binary!(impl Ge(ge) for Number -> Feature);
impl_expr_binary!(impl Gt(gt) for Number -> Feature);
impl_expr_binary!(impl Le(le) for Number -> Feature);
impl_expr_binary!(impl Lt(lt) for Number -> Feature);
impl_expr_binary!(impl And(and) for Feature -> Feature);
impl_expr_binary!(impl Or(or) for Feature -> Feature);

impl Value {
    fn to_feature(&self) -> Result<Option<Feature>> {
        match self {
            Self::Feature(value) => Ok(Some(*value)),
            Self::Number(_) => bail!("unexpected value"),
            Self::Variable(_) => Ok(None),
        }
    }

    fn to_number(&self) -> Result<Option<Number>> {
        match self {
            Self::Feature(_) => bail!("unexpected feature"),
            Self::Number(value) => Ok(Some(*value)),
            Self::Variable(_) => Ok(None),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Feature(bool);

impl Feature {
    pub const fn new(value: bool) -> Self {
        Self(value)
    }

    pub const fn into_inner(self) -> bool {
        self.0
    }
}

impl Not for Feature {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(self.0.not())
    }
}

impl And for Feature {
    type Output = Self;

    fn and(self, rhs: Self) -> Self::Output {
        Self(self.0 && rhs.0)
    }
}

impl Or for Feature {
    type Output = Self;

    fn or(self, rhs: Self) -> Self::Output {
        Self(self.0 || rhs.0)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Number(f64);

impl Number {
    pub const fn new(value: f64) -> Self {
        Self(value)
    }

    pub const fn into_inner(self) -> f64 {
        self.0
    }
}

impl Neg for Number {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(self.0.neg())
    }
}

impl Add for Number {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.add(rhs.0))
    }
}

impl Sub for Number {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0.sub(rhs.0))
    }
}

impl Mul for Number {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0.mul(rhs.0))
    }
}

impl Div for Number {
    type Output = Result<Self>;

    fn div(self, rhs: Self) -> Self::Output {
        if rhs.0 != 0.0 {
            Ok(Self(self.0.div(rhs.0)))
        } else {
            bail!("cannot divide by zero")
        }
    }
}

impl Eq for Number {
    type Output = Feature;

    fn eq(self, rhs: Self) -> Self::Output {
        Feature(self.0.eq(&rhs.0))
    }
}

impl Ne for Number {
    type Output = Feature;

    fn ne(self, rhs: Self) -> Self::Output {
        Feature(self.0.ne(&rhs.0))
    }
}

impl Ge for Number {
    type Output = Feature;

    fn ge(self, rhs: Self) -> Self::Output {
        Feature(self.0.ge(&rhs.0))
    }
}

impl Gt for Number {
    type Output = Feature;

    fn gt(self, rhs: Self) -> Self::Output {
        Feature(self.0.gt(&rhs.0))
    }
}

impl Le for Number {
    type Output = Feature;

    fn le(self, rhs: Self) -> Self::Output {
        Feature(self.0.le(&rhs.0))
    }
}

impl Lt for Number {
    type Output = Feature;

    fn lt(self, rhs: Self) -> Self::Output {
        Feature(self.0.lt(&rhs.0))
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum BinaryExpr {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Ge,
    Gt,
    Le,
    Lt,
    And,
    Or,
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum UnaryExpr {
    Neg,
    Not,
}
