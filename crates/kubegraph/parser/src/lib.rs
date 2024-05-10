use kubegraph_api::vm::{BinaryExpr, Number, UnaryExpr};
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
    Identity {
        value: Value,
    },
    Unary {
        value: Box<Expr>,
        op: UnaryExpr,
    },
    //
    // binary
    //
    Binary {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        op: BinaryExpr,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Value {
    Number(Number),
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
