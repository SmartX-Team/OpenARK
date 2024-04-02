use lalrpop_util::lalrpop_mod;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

lalrpop_mod!(grammar);

pub use self::grammar::{FilterParser, ProvideParser, ScriptParser};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Script(pub Vec<Stmt>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Stmt {
    Set { lhs: Literal, rhs: Expr },
    // If {
    //     r#if: Expr,
    //     then: Vec<Stmt>,
    //     r#else: Option<Vec<Stmt>>,
    // },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Filter {
    Ensure { value: Literal },
    Expr { value: Expr },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Expr {
    //
    // unary
    //
    Identity { value: Value },
    Negative { value: Box<Expr> },
    Not { value: Box<Expr> },
    //
    // binary
    //
    Mul { lhs: Box<Expr>, rhs: Box<Expr> },
    Div { lhs: Box<Expr>, rhs: Box<Expr> },
    Add { lhs: Box<Expr>, rhs: Box<Expr> },
    Sub { lhs: Box<Expr>, rhs: Box<Expr> },
    Eq { lhs: Box<Expr>, rhs: Box<Expr> },
    Ge { lhs: Box<Expr>, rhs: Box<Expr> },
    Gt { lhs: Box<Expr>, rhs: Box<Expr> },
    Le { lhs: Box<Expr>, rhs: Box<Expr> },
    Lt { lhs: Box<Expr>, rhs: Box<Expr> },
    Feature { lhs: Literal, rhs: Literal },
    And { lhs: Box<Expr>, rhs: Box<Expr> },
    Or { lhs: Box<Expr>, rhs: Box<Expr> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Value {
    Number(f64),
    Variable(Literal),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Provide {
    Ensure { value: Literal },
    Feature { lhs: Literal, rhs: Literal },
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct Literal(pub String);
