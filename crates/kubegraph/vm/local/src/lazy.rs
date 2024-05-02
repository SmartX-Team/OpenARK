use anyhow::Result;
use kubegraph_api::vm::{Instruction, Script};

#[derive(Clone, Default)]
pub struct LazyVirtualMachine {
    local_variables: Vec<Instruction>,
    parsers: ParserGroup,
    use_placeholders: bool,
}

impl LazyVirtualMachine {
    pub fn with_lazy_filter(input: &str) -> Result<Self> {
        let mut this = Self {
            use_placeholders: true,
            ..Default::default()
        };
        this.execute_filter(input).map(|()| this)
    }

    pub fn with_lazy_script(input: &str) -> Result<Self> {
        let mut this = Self {
            use_placeholders: true,
            ..Default::default()
        };
        this.execute_script(input).map(|()| this)
    }

    pub(crate) fn dump_script(&self) -> Script {
        Script {
            code: self.local_variables.clone(),
        }
    }
}

mod impl_call {
    use std::{
        collections::BTreeMap,
        ops::{Add, Div, Mul, Neg, Not, Sub},
    };

    use anyhow::{bail, Error, Result};
    use kubegraph_api::{
        graph::Graph,
        vm::{
            And, BinaryExpr, Eq, Feature, Ge, Gt, Instruction, Le, Lt, Ne, Number, Or, Stmt,
            UnaryExpr, Value,
        },
    };

    use crate::df::{DataFrame, DataSlice};

    impl super::LazyVirtualMachine {
        pub(crate) fn call(&self, graph: &Graph<DataFrame>) -> Result<DataFrame> {
            let Graph { edges, nodes } = graph;

            let graph = {
                let edges = match edges {
                    DataFrame::PolarsLazy(ldf) => ldf,
                };
                let nodes = match nodes {
                    DataFrame::PolarsLazy(ldf) => ldf,
                };

                let edges_src = nodes
                    .clone()
                    .select(&[::pl::lazy::dsl::all().name().prefix("src.")]);
                let edges_sink = nodes
                    .clone()
                    .select(&[::pl::lazy::dsl::all().name().prefix("sink.")]);

                let graph = edges
                    .clone()
                    .inner_join(
                        edges_src,
                        ::pl::lazy::dsl::col("src"),
                        ::pl::lazy::dsl::col("src.name"),
                    )
                    .inner_join(
                        edges_sink,
                        ::pl::lazy::dsl::col("sink"),
                        ::pl::lazy::dsl::col("sink.name"),
                    );

                DataFrame::PolarsLazy(graph)
            };

            let mut heap = Heap::new(graph);
            let mut stack = Stack::default();

            for (pc, ins) in self.local_variables.iter().enumerate() {
                let Instruction { name, stmt } = ins;

                // fetch from stack
                let value = match *stmt {
                    Stmt::Identity { index } if index < pc => stack.get(index),
                    Stmt::Identity { index } => {
                        bail!("illegal instruction access: {pc} -> {index}")
                    }
                    Stmt::DefineLocalFeature { value } => Variable::Feature(value),
                    Stmt::DefineLocalValue { value } => Variable::Number(value),
                    Stmt::BinaryExpr { lhs, rhs, op } => {
                        let lhs = stack.fetch(lhs);
                        let rhs = stack.fetch(rhs);
                        lhs.execute_expr_binary(op, rhs)?
                    }
                    Stmt::UnaryExpr { src, op } => {
                        let src = stack.fetch(src);
                        src.execute_expr_unary(op)?
                    }
                };

                // fetch from heap
                let value = match name {
                    Some(name) => match value {
                        Variable::Feature(None) => heap.get_feature(name)?,
                        Variable::Number(None) => heap.get_number(name)?,
                        value => {
                            heap.insert(name.clone(), value.clone())?;
                            value
                        }
                    },
                    None => value,
                };

                // store
                stack.push(value);
            }

            Ok(heap.into_graph())
        }
    }

    #[derive(Clone)]
    struct Heap {
        graph: DataFrame,
        variables: BTreeMap<String, Variable>,
    }

    impl Heap {
        fn new(graph: DataFrame) -> Self {
            Self {
                graph,
                variables: BTreeMap::default(),
            }
        }

        fn get_feature(&self, key: &str) -> Result<Variable> {
            match self.get_unchecked(key) {
                Variable::Number(_) => bail!("unexpected value: {key:?}"),
                value => Ok(value),
            }
        }

        fn get_number(&self, key: &str) -> Result<Variable> {
            match self.get_unchecked(key) {
                Variable::Feature(_) => bail!("unexpected feature: {key:?}"),
                value => Ok(value),
            }
        }

        fn get_unchecked(&self, key: &str) -> Variable {
            self.variables
                .get(key)
                .cloned()
                .unwrap_or_else(|| Variable::DataSlice(self.graph.get_column(key)))
        }

        fn insert(&mut self, key: String, value: Variable) -> Result<()> {
            match &value {
                Variable::DataSlice(column) => {
                    self.graph.insert_column(&key, column.clone());
                }
                Variable::Feature(Some(value)) => {
                    self.graph.fill_column_with_feature(&key, *value);
                }
                Variable::Feature(None) => error_undefined_feature()?,
                Variable::Number(Some(value)) => {
                    self.graph.fill_column_with_value(&key, *value);
                }
                Variable::Number(None) => error_undefined_number()?,
            }
            self.variables.insert(key, value);
            Ok(())
        }

        fn into_graph(self) -> DataFrame {
            self.graph
        }
    }

    #[derive(Default)]
    struct Stack(Vec<Variable>);

    impl Stack {
        fn get(&mut self, index: usize) -> Variable {
            self.0[index].clone()
        }

        fn fetch(&mut self, value: Value) -> Variable {
            match value {
                Value::Feature(value) => Variable::Feature(Some(value)),
                Value::Number(value) => Variable::Number(Some(value)),
                Value::Variable(index) => self.get(index),
            }
        }

        fn push(&mut self, value: Variable) {
            self.0.push(value)
        }
    }

    #[derive(Clone)]
    enum Variable {
        DataSlice(DataSlice),
        Feature(Option<Feature>),
        Number(Option<Number>),
    }

    impl From<DataSlice> for Variable {
        fn from(value: DataSlice) -> Self {
            Self::DataSlice(value)
        }
    }

    impl TryFrom<Variable> for DataSlice {
        type Error = Error;

        fn try_from(value: Variable) -> Result<Self, <Self as TryFrom<Variable>>::Error> {
            match value {
                Variable::DataSlice(value) => Ok(value),
                _ => bail!("unexpected variable"),
            }
        }
    }

    impl Variable {
        fn execute_expr_unary(self, op: UnaryExpr) -> Result<Self> {
            match op {
                UnaryExpr::Neg => self.neg(),
                UnaryExpr::Not => self.not(),
            }
        }

        fn execute_expr_binary(self, op: BinaryExpr, rhs: Self) -> Result<Self> {
            match op {
                BinaryExpr::Add => self.add(rhs),
                BinaryExpr::Sub => self.sub(rhs),
                BinaryExpr::Mul => self.mul(rhs),
                BinaryExpr::Div => self.div(rhs),
                BinaryExpr::Eq => self.eq(rhs),
                BinaryExpr::Ne => self.ne(rhs),
                BinaryExpr::Ge => self.ge(rhs),
                BinaryExpr::Gt => self.gt(rhs),
                BinaryExpr::Le => self.le(rhs),
                BinaryExpr::Lt => self.lt(rhs),
                BinaryExpr::And => self.and(rhs),
                BinaryExpr::Or => self.or(rhs),
            }
        }
    }

    macro_rules! impl_expr_unary {
        ( impl $fn:ident for $src:ident as Feature ) => {{
            match $src {
                Variable::DataSlice(src) => Ok(Variable::DataSlice(src.not())),
                Variable::Feature(Some(src)) => Ok(Variable::Feature(Some(src.not()))),
                Variable::Feature(None) => error_undefined_feature(),
                Variable::Number(src) => error_unexpected_type_number(),
            }
        }};
        ( impl $fn:ident for $src:ident as Number ) => {{
            match $src {
                Variable::DataSlice(src) => Ok(Variable::DataSlice(src.neg())),
                Variable::Feature(_) => error_unexpected_type_feature(),
                Variable::Number(Some(src)) => Ok(Variable::Number(Some(src.neg()))),
                Variable::Number(None) => error_undefined_number(),
            }
        }};
    }

    impl Neg for Variable {
        type Output = Result<Self>;

        fn neg(self) -> Self::Output {
            impl_expr_unary!(impl neg for self as Number)
        }
    }

    impl Not for Variable {
        type Output = Result<Self>;

        fn not(self) -> Self::Output {
            impl_expr_unary!(impl not for self as Feature)
        }
    }

    macro_rules! impl_expr_binary {
        ( impl $ty:ident ( $fn:ident ) for Feature -> Feature ) => {
            impl $ty for Variable {
                type Output = Result<Self>;

                fn $fn(self, rhs: Self) -> Self::Output {
                    match self {
                        Variable::DataSlice(lhs) => match rhs {
                            Variable::DataSlice(rhs) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Feature(Some(rhs)) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Feature(None) => error_undefined_feature(),
                            Variable::Number(_) => error_unexpected_type_number(),
                        },
                        Variable::Feature(Some(lhs)) => match rhs {
                            Variable::DataSlice(rhs) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Feature(Some(rhs)) => {
                                Ok(Variable::Feature(Some(lhs.$fn(rhs))))
                            }
                            Variable::Feature(None) => error_undefined_feature(),
                            Variable::Number(_) => error_unexpected_type_number(),
                        },
                        Variable::Feature(None) => error_undefined_feature(),
                        Variable::Number(_) => error_unexpected_type_number(),
                    }
                }
            }
        };
        ( impl $ty:ident ( $fn:ident ) for Number -> Feature ) => {
            impl $ty for Variable {
                type Output = Result<Self>;

                fn $fn(self, rhs: Self) -> Self::Output {
                    match self {
                        Variable::DataSlice(lhs) => match rhs {
                            Variable::DataSlice(rhs) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Feature(_) => error_unexpected_type_feature(),
                            Variable::Number(Some(rhs)) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Number(None) => error_undefined_number(),
                        },
                        Variable::Feature(_) => error_unexpected_type_feature(),
                        Variable::Number(Some(lhs)) => match rhs {
                            Variable::DataSlice(rhs) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Feature(_) => error_unexpected_type_feature(),
                            Variable::Number(Some(rhs)) => {
                                Ok(Variable::Feature(Some(lhs.$fn(rhs))))
                            }
                            Variable::Number(None) => error_undefined_number(),
                        },
                        Variable::Number(None) => error_undefined_number(),
                    }
                }
            }
        };
        ( impl $ty:ident ( $fn:ident ) for Number -> Number ) => {
            impl $ty for Variable {
                type Output = Result<Self>;

                fn $fn(self, rhs: Self) -> Self::Output {
                    match self {
                        Variable::DataSlice(lhs) => match rhs {
                            Variable::DataSlice(rhs) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Feature(_) => error_unexpected_type_feature(),
                            Variable::Number(Some(rhs)) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Number(None) => error_undefined_number(),
                        },
                        Variable::Feature(_) => error_unexpected_type_feature(),
                        Variable::Number(Some(lhs)) => match rhs {
                            Variable::DataSlice(rhs) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Feature(_) => error_unexpected_type_feature(),
                            Variable::Number(Some(rhs)) => Ok(Variable::Number(Some(lhs.$fn(rhs)))),
                            Variable::Number(None) => error_undefined_number(),
                        },
                        Variable::Number(None) => error_undefined_number(),
                    }
                }
            }
        };
        ( impl $ty:ident ( $fn:ident ) for Number -> Number? ) => {
            impl $ty for Variable {
                type Output = Result<Self>;

                fn $fn(self, rhs: Self) -> Self::Output {
                    match self {
                        Variable::DataSlice(lhs) => match rhs {
                            Variable::DataSlice(rhs) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Feature(_) => error_unexpected_type_feature(),
                            Variable::Number(Some(rhs)) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Number(None) => error_undefined_number(),
                        },
                        Variable::Feature(_) => error_unexpected_type_feature(),
                        Variable::Number(Some(lhs)) => match rhs {
                            Variable::DataSlice(rhs) => Ok(Variable::DataSlice(lhs.$fn(rhs))),
                            Variable::Feature(_) => error_unexpected_type_feature(),
                            Variable::Number(Some(rhs)) => {
                                Ok(Variable::Number(Some(lhs.$fn(rhs)?)))
                            }
                            Variable::Number(None) => error_undefined_number(),
                        },
                        Variable::Number(None) => error_undefined_number(),
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

    fn error_undefined<T>(kind: &str) -> Result<T> {
        bail!("undefined {kind}")
    }

    fn error_undefined_feature<T>() -> Result<T> {
        error_undefined("feature")
    }

    fn error_undefined_number<T>() -> Result<T> {
        error_undefined("number")
    }

    fn error_unexpected_type<T>(kind: &str) -> Result<T> {
        bail!("unexpected {kind}")
    }

    fn error_unexpected_type_feature<T>() -> Result<T> {
        error_unexpected_type("feature")
    }

    fn error_unexpected_type_number<T>() -> Result<T> {
        error_unexpected_type("number")
    }
}

mod impl_execute {
    use anyhow::{anyhow, Result};
    use kubegraph_api::vm::{
        BinaryExpr, Feature, Instruction, Number, Stmt as LazyStmt, UnaryExpr, Value as RefValue,
    };
    use kubegraph_parser::{Expr, Filter, Literal, Script, Stmt, Value};

    impl super::LazyVirtualMachine {
        pub fn execute_script(&mut self, input: &str) -> Result<()> {
            let Script(stmts) = self
                .parsers
                .script
                .parse(input)
                .map_err(|error| anyhow!("{error}"))?;

            stmts
                .into_iter()
                .try_for_each(|stmt| self.execute_stmt(stmt))
        }

        pub fn execute_filter(&mut self, input: &str) -> Result<()> {
            let filter = self
                .parsers
                .filter
                .parse(input)
                .map_err(|error| anyhow!("{error}"))?;

            match filter {
                Filter::Ensure {
                    value: Literal(name),
                } => {
                    self.execute_register_value(name, None);
                    Ok(())
                }
                Filter::Expr { value: expr } => self.execute_expr(expr).map(|_| ()),
            }
        }

        fn execute_stmt(&mut self, stmt: Stmt) -> Result<()> {
            match stmt {
                Stmt::Set { lhs, rhs } => {
                    let ins = Instruction {
                        name: Some(lhs.0),
                        stmt: self.execute_expr(rhs)?.into(),
                    };
                    self.execute_register_instruction(ins);
                    Ok(())
                }
            }
        }

        pub(crate) fn execute_register_feature(
            &mut self,
            name: String,
            value: Option<Feature>,
        ) -> RefValue {
            let ins = Instruction {
                name: Some(name),
                stmt: value.into(),
            };
            self.execute_register_instruction(ins)
        }

        pub(crate) fn execute_register_value(
            &mut self,
            name: String,
            value: Option<Number>,
        ) -> RefValue {
            let ins = Instruction {
                name: Some(name),
                stmt: value.into(),
            };
            self.execute_register_instruction(ins)
        }

        pub(crate) fn execute_register_instruction(&mut self, ins: Instruction) -> RefValue {
            let index = self.local_variables.len();
            self.local_variables.push(ins);
            RefValue::Variable(index)
        }

        fn execute_get_local_value(&mut self, value: Value) -> Result<RefValue> {
            match value {
                Value::Number(data) => Ok(RefValue::Number(data)),
                Value::Variable(name) => self.execute_get_local_value_by_name(&name.0),
            }
        }

        fn execute_get_local_value_by_index(&mut self, index: usize) -> Result<RefValue> {
            self.local_variables
                .get(index)
                .map(|ins| ins.stmt.to_value().unwrap_or(RefValue::Variable(index)))
                .or_else(|| self.try_register_value(format!("%{index}")))
                .ok_or_else(|| anyhow!("undefined local value {index:?}"))
        }

        fn execute_get_local_value_by_name(&mut self, name: &str) -> Result<RefValue> {
            self.local_variables
                .iter()
                .enumerate()
                .find(|&(_, ins)| ins.name.as_ref().map(|x| x.as_str()) == Some(name))
                .map(|(index, ins)| ins.stmt.to_value().unwrap_or(RefValue::Variable(index)))
                .or_else(|| self.try_register_value(name))
                .ok_or_else(|| anyhow!("undefined local value named {name:?}"))
        }

        fn try_register_value(&mut self, name: impl ToString) -> Option<RefValue> {
            if self.use_placeholders {
                Some(self.execute_register_value(name.to_string(), None))
            } else {
                None
            }
        }

        fn execute_expr(&mut self, expr: Expr) -> Result<RefValue> {
            let stmt = match expr {
                Expr::Identity { value } => return self.execute_get_local_value(value),
                Expr::Unary { value, op } => self.execute_expr_unary(op, *value)?,
                Expr::Binary { lhs, rhs, op } => self.execute_expr_binary(op, *lhs, *rhs)?,
                Expr::Feature { lhs, rhs } => self.execute_expr_feature(lhs.0, rhs.0)?,
            };

            match stmt.to_value() {
                Some(value) => Ok(value),
                None => {
                    let ins = Instruction { name: None, stmt };
                    Ok(self.execute_register_instruction(ins))
                }
            }
        }

        fn execute_expr_unary(&mut self, op: UnaryExpr, value: Expr) -> Result<LazyStmt> {
            match op {
                UnaryExpr::Neg => self.execute_expr_unary_neg(value),
                UnaryExpr::Not => self.execute_expr_unary_not(value),
            }
        }

        fn execute_expr_unary_neg(&mut self, src: Expr) -> Result<LazyStmt> {
            use std::ops::Neg;

            self.execute_expr(src).and_then(|value| value.neg())
        }

        fn execute_expr_unary_not(&mut self, src: Expr) -> Result<LazyStmt> {
            use std::ops::Not;

            self.execute_expr(src).and_then(|value| value.not())
        }

        fn execute_expr_binary(
            &mut self,
            op: BinaryExpr,
            lhs: Expr,
            rhs: Expr,
        ) -> Result<LazyStmt> {
            match op {
                BinaryExpr::Add => self.execute_expr_binary_add(lhs, rhs),
                BinaryExpr::Sub => self.execute_expr_binary_sub(lhs, rhs),
                BinaryExpr::Mul => self.execute_expr_binary_mul(lhs, rhs),
                BinaryExpr::Div => self.execute_expr_binary_div(lhs, rhs),
                BinaryExpr::Eq => self.execute_expr_binary_eq(lhs, rhs),
                BinaryExpr::Ne => self.execute_expr_binary_ne(lhs, rhs),
                BinaryExpr::Ge => self.execute_expr_binary_ge(lhs, rhs),
                BinaryExpr::Gt => self.execute_expr_binary_gt(lhs, rhs),
                BinaryExpr::Le => self.execute_expr_binary_le(lhs, rhs),
                BinaryExpr::Lt => self.execute_expr_binary_lt(lhs, rhs),
                BinaryExpr::And => self.execute_expr_binary_and(lhs, rhs),
                BinaryExpr::Or => self.execute_expr_binary_or(lhs, rhs),
            }
        }

        fn execute_expr_binary_add(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use std::ops::Add;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.add(rhs)
        }

        fn execute_expr_binary_sub(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use std::ops::Sub;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.sub(rhs)
        }

        fn execute_expr_binary_mul(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use std::ops::Mul;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.mul(rhs)
        }

        fn execute_expr_binary_div(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use std::ops::Div;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.div(rhs)
        }

        fn execute_expr_binary_eq(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use kubegraph_api::vm::Eq;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.eq(rhs)
        }

        fn execute_expr_binary_ne(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use kubegraph_api::vm::Ne;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.ne(rhs)
        }

        fn execute_expr_binary_ge(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use kubegraph_api::vm::Ge;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.ge(rhs)
        }

        fn execute_expr_binary_gt(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use kubegraph_api::vm::Gt;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.gt(rhs)
        }

        fn execute_expr_binary_le(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use kubegraph_api::vm::Le;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.le(rhs)
        }

        fn execute_expr_binary_lt(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use kubegraph_api::vm::Lt;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.lt(rhs)
        }

        fn execute_expr_binary_and(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use kubegraph_api::vm::And;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.and(rhs)
        }

        fn execute_expr_binary_or(&mut self, lhs: Expr, rhs: Expr) -> Result<LazyStmt> {
            use kubegraph_api::vm::Or;

            let lhs = self.execute_expr(lhs)?;
            let rhs = self.execute_expr(rhs)?;
            lhs.or(rhs)
        }

        fn execute_expr_feature(&mut self, lhs: String, rhs: String) -> Result<LazyStmt> {
            // TODO: to be implemented
            todo!()
        }
    }
}

#[derive(Default)]
struct ParserGroup {
    filter: ::kubegraph_parser::FilterParser,
    script: ::kubegraph_parser::ScriptParser,
}

impl Clone for ParserGroup {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use kubegraph_api::vm::{BinaryExpr, Number, Stmt, Value};

    use super::*;

    #[test]
    fn lazy_simple_add() {
        let mut vm = LazyVirtualMachine::default();

        let input = "a = 1 + 2; b = 3 + 4; c = a + b;";
        vm.execute_script(input).expect("failed to compile");

        let script = vm.dump_script();

        assert_eq!(
            script.code,
            &[
                Instruction {
                    name: Some("a".into()),
                    stmt: Stmt::DefineLocalValue {
                        value: Some(Number::new(3.)),
                    },
                },
                Instruction {
                    name: Some("b".into()),
                    stmt: Stmt::DefineLocalValue {
                        value: Some(Number::new(7.)),
                    },
                },
                Instruction {
                    name: Some("c".into()),
                    stmt: Stmt::DefineLocalValue {
                        value: Some(Number::new(10.)),
                    },
                },
            ]
        );
    }

    #[test]
    fn lazy_simple_add_with_placeholder() {
        let mut vm = LazyVirtualMachine::default();
        vm.execute_register_value("a".into(), None);

        let input = "b = 3 + 4; c = a + b;";
        vm.execute_script(input).expect("failed to compile");

        let script = vm.dump_script();

        assert_eq!(
            script.code,
            &[
                Instruction {
                    name: Some("a".into()),
                    stmt: Stmt::DefineLocalValue { value: None },
                },
                Instruction {
                    name: Some("b".into()),
                    stmt: Stmt::DefineLocalValue {
                        value: Some(Number::new(7.)),
                    },
                },
                Instruction {
                    name: None,
                    stmt: Stmt::BinaryExpr {
                        lhs: Value::Variable(0),
                        rhs: Value::Number(Number::new(7.)),
                        op: BinaryExpr::Add,
                    },
                },
                Instruction {
                    name: Some("c".into()),
                    stmt: Stmt::Identity { index: 2 },
                },
            ]
        );
    }
}
