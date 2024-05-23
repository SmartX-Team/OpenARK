use std::{
    ops::{Add, Div, Mul, Neg, Not, Sub},
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::{stream::FuturesUnordered, TryStreamExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Instant};
use tracing::{error, info, instrument, warn, Level};

use crate::{
    analyzer::{NetworkAnalyzer, NetworkAnalyzerExt},
    frame::LazyFrame,
    graph::{
        Graph, GraphData, GraphEdges, GraphMetadata, GraphMetadataPinnedExt, GraphMetadataStandard,
        GraphScope, NetworkGraphDB, NetworkGraphDBExt, ScopedNetworkGraphDBContainer,
    },
    ops::{And, Eq, Ge, Gt, Le, Lt, Ne, Or},
    problem::VirtualProblem,
    resource::NetworkResourceCollectionDB,
    runner::NetworkRunner,
    solver::NetworkSolver,
};

#[async_trait]
pub trait NetworkVirtualMachineExt
where
    Self: NetworkVirtualMachine,
{
    #[instrument(level = Level::INFO, skip(self))]
    async fn loop_forever(&self) {
        let fallback_interval = self.fallback_interval();

        loop {
            match self.try_loop_forever().await {
                Ok(()) => break,
                Err(error) => {
                    error!("failed to operate kubegraph VM: {error}");

                    warn!("restarting VM in {fallback_interval:?}...");
                    sleep(fallback_interval).await
                }
            }
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn try_loop_forever(&self) -> Result<()> {
        info!("Starting kubegraph VM...");

        loop {
            let instant = Instant::now();

            self.step().await?;

            if let Some(interval) = self.interval() {
                let elapsed = instant.elapsed() + Duration::from_micros(500);
                if elapsed < interval {
                    sleep(interval - elapsed).await;
                }
            }
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn step(&self) -> Result<()> {
        // Define-or-Reuse a converged problem
        let problems = self.analyzer().inspect(self.resource_db()).await?;

        // Apply it
        problems
            .into_iter()
            .map(|problem| self.step_with_custom_problem(problem))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn step_with_custom_problem(&self, problem: VirtualProblem) -> Result<()> {
        // Step 1. Pull & Convert nodes (and "static" edges)
        let GraphData { edges, nodes } = match self.pull_graph(&problem).await? {
            Some(graph) => graph,
            None => return Ok(()),
        };

        // Step 2. Infer edges by functions
        let function_edges = self.infer_edges(&problem, nodes.clone()).await?;
        let static_edges = GraphEdges::from_static(
            &problem.scope.namespace,
            &problem.spec.metadata.function(),
            edges,
        )?;

        let edges = match (function_edges, static_edges.clone()) {
            (Some(function_edges), Some(static_edges)) => function_edges.concat(static_edges)?,
            (Some(function_edges), None) => function_edges,
            (None, Some(static_edges)) => static_edges,
            (None, None) => return Ok(()),
        };

        let graph = GraphData {
            edges: edges.into_inner(),
            nodes: nodes.clone(),
        };

        // Step 3. Solve edge flows
        let graph = self.solver().solve(graph, &problem.spec).await?;

        // Step 4. Apply edges to real-world (or simulator)
        let graph_db_scope = GraphScope {
            namespace: problem.scope.namespace.clone(),
            name: GraphScope::NAME_GLOBAL.into(),
        };
        let graph_db_scoped = ScopedNetworkGraphDBContainer {
            inner: self.graph_db(),
            metadata: &GraphMetadata::Standard(problem.spec.metadata),
            scope: &graph_db_scope,
            static_edges,
        };
        self.runner()
            .execute(&graph_db_scoped, graph, &problem.spec)
            .await
    }

    #[instrument(level = Level::INFO, skip(self, problem))]
    async fn pull_graph(&self, problem: &VirtualProblem) -> Result<Option<GraphData<LazyFrame>>> {
        let VirtualProblem { filter, scope, .. } = problem;

        // If there is a global graph, use this
        if let Some(graph) = self
            .graph_db()
            .get_global_namespaced(&scope.namespace)
            .await?
        {
            let graph = self.analyzer().pin_graph(problem, graph).await?;
            return Ok(Some(graph.data));
        }

        let graphs = self.graph_db().list(filter).await?;
        if graphs.is_empty() {
            return Ok(None);
        }

        graphs
            .into_iter()
            .map(|graph| self.analyzer().pin_graph(problem, graph))
            .collect::<FuturesUnordered<_>>()
            .map_ok(
                |Graph {
                     data,
                     metadata: GraphMetadataStandard {},
                     scope: _,
                 }| data,
            )
            .try_fold(GraphData::default(), |a, b| async move { a.concat(b) })
            .await
            .map(Some)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close(&self) -> Result<()> {
        self.graph_db().close().await?;
        self.close_workers().await
    }
}

impl<T> NetworkVirtualMachineExt for T where Self: NetworkVirtualMachine {}

#[async_trait]
pub trait NetworkVirtualMachine
where
    Self: Clone + Send + Sync,
{
    type Analyzer: NetworkAnalyzer;
    type ResourceDB: 'static + Send + Clone + NetworkResourceCollectionDB;
    type GraphDB: 'static + Send + Clone + NetworkGraphDB;
    type Runner: NetworkRunner<GraphData<LazyFrame>>;
    type Solver: NetworkSolver<GraphData<LazyFrame>, Output = GraphData<LazyFrame>>;

    fn analyzer(&self) -> &<Self as NetworkVirtualMachine>::Analyzer;

    fn graph_db(&self) -> &<Self as NetworkVirtualMachine>::GraphDB;

    fn resource_db(&self) -> &<Self as NetworkVirtualMachine>::ResourceDB;

    fn runner(&self) -> &<Self as NetworkVirtualMachine>::Runner;

    fn solver(&self) -> &<Self as NetworkVirtualMachine>::Solver;

    fn fallback_interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn interval(&self) -> Option<Duration>;

    async fn infer_edges(
        &self,
        problem: &VirtualProblem,
        nodes: LazyFrame,
    ) -> Result<Option<GraphEdges<LazyFrame>>>;

    async fn close_workers(&self) -> Result<()>;
}

#[async_trait]
impl<T> NetworkVirtualMachine for Arc<T>
where
    T: ?Sized + NetworkVirtualMachine,
{
    type Analyzer = <T as NetworkVirtualMachine>::Analyzer;
    type GraphDB = <T as NetworkVirtualMachine>::GraphDB;
    type ResourceDB = <T as NetworkVirtualMachine>::ResourceDB;
    type Runner = <T as NetworkVirtualMachine>::Runner;
    type Solver = <T as NetworkVirtualMachine>::Solver;

    fn analyzer(&self) -> &<Self as NetworkVirtualMachine>::Analyzer {
        <T as NetworkVirtualMachine>::analyzer(&**self)
    }

    fn graph_db(&self) -> &<Self as NetworkVirtualMachine>::GraphDB {
        <T as NetworkVirtualMachine>::graph_db(&**self)
    }

    fn resource_db(&self) -> &<Self as NetworkVirtualMachine>::ResourceDB {
        <T as NetworkVirtualMachine>::resource_db(&**self)
    }

    fn runner(&self) -> &<Self as NetworkVirtualMachine>::Runner {
        <T as NetworkVirtualMachine>::runner(&**self)
    }

    fn solver(&self) -> &<Self as NetworkVirtualMachine>::Solver {
        <T as NetworkVirtualMachine>::solver(&**self)
    }

    fn interval(&self) -> Option<Duration> {
        <T as NetworkVirtualMachine>::interval(&**self)
    }

    #[instrument(level = Level::INFO, skip(self, problem, nodes))]
    async fn infer_edges(
        &self,
        problem: &VirtualProblem,
        nodes: LazyFrame,
    ) -> Result<Option<GraphEdges<LazyFrame>>> {
        <T as NetworkVirtualMachine>::infer_edges(&**self, problem, nodes).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close_workers(&self) -> Result<()> {
        <T as NetworkVirtualMachine>::close_workers(&**self).await
    }
}

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
