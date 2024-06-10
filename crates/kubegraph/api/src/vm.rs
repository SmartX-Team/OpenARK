use std::{
    collections::BTreeMap,
    fmt,
    ops::{Add, Div, Mul, Neg, Not, Sub},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, bail, Result};
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use duration_string::DurationString;
use futures::{stream::FuturesUnordered, TryStreamExt};
use num_traits::FromPrimitive;
use ordered_float::OrderedFloat;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Instant};
use tracing::{error, info, instrument, warn, Level};

use crate::{
    component::{NetworkComponent, NetworkComponentExt},
    dependency::{
        NetworkDependencyPipeline, NetworkDependencyPipelineTemplate, NetworkDependencySolver,
        NetworkDependencySolverSpec,
    },
    frame::LazyFrame,
    graph::{
        Graph, GraphData, GraphFilter, GraphMetadata, GraphScope, NetworkGraphDB,
        NetworkGraphDBExt, ScopedNetworkGraphDBContainer,
    },
    ops::{And, Eq, Ge, Gt, Le, Lt, Max, Min, Ne, Or},
    problem::{NetworkProblemCrd, ProblemSpec, VirtualProblem},
    resource::{NetworkResourceCollectionDB, NetworkResourceDB},
    runner::{NetworkRunner, NetworkRunnerContext},
    solver::NetworkSolver,
    visualizer::{NetworkVisualizer, NetworkVisualizerExt},
};

#[async_trait]
pub trait NetworkVirtualMachineExt
where
    Self: NetworkComponentExt + NetworkVirtualMachine,
    <Self as NetworkComponent>::Args: Parser,
{
    #[cfg(feature = "vm-entrypoint")]
    async fn main<F>(handlers: F)
    where
        Self: 'static + Sized,
        F: Send + FnOnce(&FunctionSignal, &Self) -> Vec<::tokio::task::JoinHandle<()>>,
    {
        ::ark_core::tracer::init_once();
        info!("Welcome to kubegraph!");

        let signal = FunctionSignal::default().trap_on_panic();
        if let Err(error) = signal.trap_on_sigint() {
            error!("{error}");
            return;
        }

        info!("Booting...");
        let vm = match <Self as NetworkComponentExt>::try_default(&signal).await {
            Ok(vm) => vm,
            Err(error) => {
                signal
                    .panic(anyhow!("failed to init network virtual machine: {error}"))
                    .await
            }
        };

        info!("Registering side workers...");
        let handlers = handlers(&signal, &vm);

        info!("Ready");
        signal.wait_to_terminate().await;

        info!("Terminating...");
        for handler in handlers {
            handler.abort();
        }

        if let Err(error) = vm.close().await {
            error!("{error}");
        };

        signal.exit().await
    }

    #[instrument(level = Level::INFO, skip(self, signal))]
    async fn loop_forever(&self, signal: FunctionSignal) {
        let fallback_interval = self.fallback_policy();

        loop {
            match self.try_loop_forever().await {
                Ok(()) => {
                    info!("Completed VM");
                    signal.terminate();
                    break;
                }
                Err(error) => {
                    error!("failed to operate kubegraph VM: {error}");

                    let interval = match fallback_interval {
                        NetworkVirtualMachineFallbackPolicy::Interval { interval } => interval,
                        NetworkVirtualMachineFallbackPolicy::Never => {
                            signal.terminate_on_panic();
                            break;
                        }
                    };

                    warn!("restarting VM in {interval:?}...");
                    sleep(interval).await
                }
            }
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn try_loop_forever(&self) -> Result<()> {
        info!("Starting kubegraph VM...");

        let mut state = self::sealed::NetworkVirtualMachineState::Pending;
        loop {
            let instant = Instant::now();

            state = self.step(state).await?;

            let interval = match state {
                self::sealed::NetworkVirtualMachineState::Pending => {
                    NetworkVirtualMachineRestartPolicy::DEFAULT_INTERVAL_INIT
                }
                self::sealed::NetworkVirtualMachineState::Ready
                | self::sealed::NetworkVirtualMachineState::Empty => match self.restart_policy() {
                    NetworkVirtualMachineRestartPolicy::Always => {
                        NetworkVirtualMachineRestartPolicy::DEFAULT_INTERVAL
                    }
                    NetworkVirtualMachineRestartPolicy::Interval { interval } => interval,
                    NetworkVirtualMachineRestartPolicy::Manually => {
                        self.visualizer().wait_to_next().await?;
                        continue;
                    }
                    NetworkVirtualMachineRestartPolicy::Never => {
                        NetworkVirtualMachineRestartPolicy::DEFAULT_INTERVAL_INIT
                    }
                },
                self::sealed::NetworkVirtualMachineState::Completed => {
                    match self.restart_policy() {
                        NetworkVirtualMachineRestartPolicy::Always => {
                            NetworkVirtualMachineRestartPolicy::DEFAULT_INTERVAL
                        }
                        NetworkVirtualMachineRestartPolicy::Interval { interval } => interval,
                        NetworkVirtualMachineRestartPolicy::Manually => {
                            self.visualizer().wait_to_next().await?;
                            continue;
                        }
                        NetworkVirtualMachineRestartPolicy::Never => break Ok(()),
                    }
                }
            };

            let elapsed = instant.elapsed() + Duration::from_micros(500);
            if elapsed < interval {
                sleep(interval - elapsed).await;
            }
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn step(
        &self,
        state: self::sealed::NetworkVirtualMachineState,
    ) -> Result<self::sealed::NetworkVirtualMachineState> {
        // Define-or-Reuse a converged problem
        let problems = self.pull_problems().await?;
        if problems.is_empty() {
            return Ok(self::sealed::NetworkVirtualMachineState::Ready);
        }

        // Apply it
        problems
            .into_iter()
            .map(|problem| self.step_with_custom_problem(state, problem))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn step_with_custom_problem(
        &self,
        state: self::sealed::NetworkVirtualMachineState,
        problem: VirtualProblem,
    ) -> Result<self::sealed::NetworkVirtualMachineState> {
        // Step 1. Pull & Convert graphs
        let NetworkDependencyPipeline {
            connectors,
            functions,
            template:
                NetworkDependencyPipelineTemplate {
                    graph:
                        Graph {
                            connector,
                            data,
                            metadata,
                            scope,
                        },
                    static_edges,
                },
        } = match self.pull_graph(&problem).await? {
            Some(pipeline) => match state {
                self::sealed::NetworkVirtualMachineState::Pending => {
                    self.visualizer()
                        .replace_graph(pipeline.template.graph)
                        .await?;
                    return Ok(self::sealed::NetworkVirtualMachineState::Ready);
                }
                _ => pipeline,
            },
            None => return Ok(self::sealed::NetworkVirtualMachineState::Empty),
        };

        // Step 2. Solve edge flows
        let data = self.solver().solve(data, &problem.spec).await?;

        // Step 3. Apply edges to real-world (or simulator)
        let runner_ctx = NetworkRunnerContext {
            connectors,
            functions,
            graph: data.clone(),
            graph_db: ScopedNetworkGraphDBContainer {
                inner: self.graph_db(),
                scope: &scope,
            },
            problem,
            static_edges,
        };
        self.runner().execute(runner_ctx).await?;

        // Step 4. Visualize the outputs
        let graph = Graph {
            connector,
            data,
            metadata,
            scope,
        };
        self.visualizer().replace_graph(graph).await?;
        Ok(self::sealed::NetworkVirtualMachineState::Completed)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn pull_problems(&self) -> Result<Vec<VirtualProblem>> {
        Ok(self
            .resource_db()
            .list(())
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|cr: NetworkProblemCrd| {
                let scope = GraphScope::from_resource(&cr);
                VirtualProblem {
                    filter: GraphFilter::all(scope.namespace.clone()),
                    scope,
                    spec: cr.spec,
                }
            })
            .collect())
    }

    #[instrument(level = Level::INFO, skip(self, problem))]
    async fn pull_graph(
        &self,
        problem: &VirtualProblem,
    ) -> Result<Option<NetworkDependencyPipeline<Graph<GraphData<LazyFrame>>>>> {
        let VirtualProblem {
            filter,
            scope,
            spec: ProblemSpec {
                metadata,
                verbose: _,
            },
        } = problem;

        // Step 1. Collect all graphs
        let graphs = match self
            .graph_db()
            .get_global_namespaced(&scope.namespace)
            .await?
        {
            // If there is a global graph, use this
            Some(graph) => vec![graph],
            None => self.graph_db().list(filter).await?,
        };
        if graphs.is_empty() {
            return Ok(None);
        }

        // Step 2. Collect all connectors
        // NOTE: static edges can be used instead of functions
        let connectors = graphs
            .iter()
            .filter_map(|graph| graph.connector.clone())
            .map(|cr| (GraphScope::from_resource(&*cr), cr))
            .collect();

        // Step 3. Collect all functions
        // NOTE: static edges can be used instead of functions
        let functions: BTreeMap<_, _> = self
            .resource_db()
            .list(())
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|cr| (GraphScope::from_resource(&cr), cr))
            .collect();

        // Step 4. Solve the dependencies
        let spec = NetworkDependencySolverSpec {
            functions: functions.clone(),
            graphs,
        };
        let NetworkDependencyPipelineTemplate {
            graph: data,
            static_edges,
        } = self
            .dependency_solver()
            .build_pipeline(problem, spec)
            .await?;

        Ok(Some(NetworkDependencyPipeline {
            connectors,
            functions,
            template: NetworkDependencyPipelineTemplate {
                graph: Graph {
                    connector: None,
                    data,
                    metadata: GraphMetadata::Pinned(metadata.clone()),
                    scope: GraphScope {
                        namespace: scope.namespace.clone(),
                        name: GraphScope::NAME_GLOBAL.into(),
                    },
                },
                static_edges,
            },
        }))
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close(&self) -> Result<()> {
        self.graph_db().close().await?;
        self.close_workers().await
    }
}

impl<T> NetworkVirtualMachineExt for T
where
    Self: NetworkComponentExt + NetworkVirtualMachine,
    <Self as NetworkComponent>::Args: Parser,
{
}

mod sealed {
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
    pub enum NetworkVirtualMachineState {
        Pending,
        Ready,
        Empty,
        #[default]
        Completed,
    }

    impl Extend<Self> for NetworkVirtualMachineState {
        fn extend<T: IntoIterator<Item = Self>>(&mut self, iter: T) {
            *self = iter.into_iter().min().unwrap_or(*self)
        }
    }
}

#[async_trait]
pub trait NetworkVirtualMachine
where
    Self: Send + Sync,
{
    type DependencySolver: NetworkComponent + NetworkDependencySolver;
    type ResourceDB: 'static + Send + Clone + NetworkComponent + NetworkResourceCollectionDB;
    type GraphDB: 'static + Send + Clone + NetworkComponent + NetworkGraphDB;
    type Runner: NetworkComponent
        + for<'a> NetworkRunner<<Self as NetworkVirtualMachine>::GraphDB, LazyFrame>;
    type Solver: NetworkComponent
        + NetworkSolver<GraphData<LazyFrame>, Output = GraphData<LazyFrame>>;
    type Visualizer: NetworkComponent + NetworkVisualizer;

    fn dependency_solver(&self) -> &<Self as NetworkVirtualMachine>::DependencySolver;

    fn graph_db(&self) -> &<Self as NetworkVirtualMachine>::GraphDB;

    fn resource_db(&self) -> &<Self as NetworkVirtualMachine>::ResourceDB;

    fn runner(&self) -> &<Self as NetworkVirtualMachine>::Runner;

    fn solver(&self) -> &<Self as NetworkVirtualMachine>::Solver;

    fn visualizer(&self) -> &<Self as NetworkVirtualMachine>::Visualizer;

    fn fallback_policy(&self) -> NetworkVirtualMachineFallbackPolicy {
        NetworkVirtualMachineFallbackPolicy::default()
    }

    fn restart_policy(&self) -> NetworkVirtualMachineRestartPolicy {
        NetworkVirtualMachineRestartPolicy::default()
    }

    async fn close_workers(&self) -> Result<()>;
}

#[async_trait]
impl<T> NetworkVirtualMachine for Arc<T>
where
    T: ?Sized + NetworkVirtualMachine,
{
    type DependencySolver = <T as NetworkVirtualMachine>::DependencySolver;
    type GraphDB = <T as NetworkVirtualMachine>::GraphDB;
    type ResourceDB = <T as NetworkVirtualMachine>::ResourceDB;
    type Runner = <T as NetworkVirtualMachine>::Runner;
    type Solver = <T as NetworkVirtualMachine>::Solver;
    type Visualizer = <T as NetworkVirtualMachine>::Visualizer;

    fn dependency_solver(&self) -> &<Self as NetworkVirtualMachine>::DependencySolver {
        <T as NetworkVirtualMachine>::dependency_solver(&**self)
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

    fn visualizer(&self) -> &<Self as NetworkVirtualMachine>::Visualizer {
        <T as NetworkVirtualMachine>::visualizer(&**self)
    }

    fn fallback_policy(&self) -> NetworkVirtualMachineFallbackPolicy {
        <T as NetworkVirtualMachine>::fallback_policy(&**self)
    }

    fn restart_policy(&self) -> NetworkVirtualMachineRestartPolicy {
        <T as NetworkVirtualMachine>::restart_policy(&**self)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close_workers(&self) -> Result<()> {
        <T as NetworkVirtualMachine>::close_workers(&**self).await
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum NetworkVirtualMachineFallbackPolicy<T = Duration> {
    Interval { interval: T },
    Never,
}

impl NetworkVirtualMachineFallbackPolicy {
    pub const DEFAULT_INTERVAL: Duration = NetworkVirtualMachineRestartPolicy::DEFAULT_INTERVAL;
}

impl Default for NetworkVirtualMachineFallbackPolicy {
    fn default() -> Self {
        Self::Interval {
            interval: Self::DEFAULT_INTERVAL,
        }
    }
}

impl FromStr for NetworkVirtualMachineFallbackPolicy {
    type Err = ::duration_string::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Never" | "never" | "False" | "false" | "No" | "no" => Ok(Self::Never),
            s => DurationString::from_str(s).map(|interval| Self::Interval {
                interval: interval.into(),
            }),
        }
    }
}

impl fmt::Display for NetworkVirtualMachineFallbackPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkVirtualMachineFallbackPolicy::Interval { interval } => {
                fmt::Debug::fmt(interval, f)
            }
            NetworkVirtualMachineFallbackPolicy::Never => "Never".fmt(f),
        }
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum NetworkVirtualMachineRestartPolicy<T = Duration> {
    Always,
    Interval { interval: T },
    Manually,
    Never,
}

impl NetworkVirtualMachineRestartPolicy {
    pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(5);
    pub const DEFAULT_INTERVAL_INIT: Duration = Duration::from_millis(200);
}

impl Default for NetworkVirtualMachineRestartPolicy {
    fn default() -> Self {
        Self::Interval {
            interval: Self::DEFAULT_INTERVAL,
        }
    }
}

impl FromStr for NetworkVirtualMachineRestartPolicy {
    type Err = ::duration_string::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Always" | "always" | "True" | "true" | "Yes" | "yes" => Ok(Self::Always),
            "Manually" | "manually" | "Manual" | "manual" => Ok(Self::Manually),
            "Never" | "never" | "False" | "false" | "No" | "no" => Ok(Self::Never),
            s => DurationString::from_str(s).map(|interval| Self::Interval {
                interval: interval.into(),
            }),
        }
    }
}

impl fmt::Display for NetworkVirtualMachineRestartPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkVirtualMachineRestartPolicy::Always => "Always".fmt(f),
            NetworkVirtualMachineRestartPolicy::Interval { interval } => {
                fmt::Debug::fmt(interval, f)
            }
            NetworkVirtualMachineRestartPolicy::Manually => "Manually".fmt(f),
            NetworkVirtualMachineRestartPolicy::Never => "Never".fmt(f),
        }
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

#[derive(Clone, Debug, PartialEq)]
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
    FunctionExpr {
        op: FunctionExpr,
        args: Vec<Value>,
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
            Stmt::FunctionExpr { .. } => None,
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
    ( impl $name:ident ($fn:ident) for $src:ident as Feature -> Feature ) => {
        impl $name for Value {
            type Output = Result<Stmt>;

            fn $fn(self) -> Self::Output {
                match self.to_feature()? {
                    Some(src) => Ok(Stmt::DefineLocalFeature {
                        value: Some(src.$fn()),
                    }),
                    _ => Ok(Stmt::UnaryExpr {
                        src: self,
                        op: UnaryExpr::$name,
                    }),
                }
            }
        }
    };
    ( impl $name:ident ($fn:ident) for $src:ident as Number -> Number ) => {
        impl $name for Value {
            type Output = Result<Stmt>;

            fn $fn(self) -> Self::Output {
                match self.to_number()? {
                    Some(src) => Ok(Stmt::DefineLocalValue {
                        value: Some(src.$fn()),
                    }),
                    _ => Ok(Stmt::UnaryExpr {
                        src: self,
                        op: UnaryExpr::$name,
                    }),
                }
            }
        }
    };
}

impl_expr_unary!(impl Neg(neg) for self as Number -> Number);
impl_expr_unary!(impl Not(not) for self as Feature -> Feature);

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

macro_rules! impl_expr_function_builtin {
    ( impl $name:ident ($fn:ident) for $args:ident as Number -> Number ) => {
        impl $name for Vec<Value> {
            type Output = Result<Stmt>;

            fn $fn(self) -> Self::Output {
                if self.iter().all(|value| value.is_number()) {
                    Ok(Stmt::DefineLocalValue {
                        value: self
                            .into_iter()
                            .filter_map(|value| value.to_number_opt())
                            .$fn(),
                    })
                } else {
                    Ok(Stmt::FunctionExpr {
                        op: FunctionExpr::BuiltIn(BuiltInFunctionExpr::$name),
                        args: self,
                    })
                }
            }
        }

        impl $name for Vec<Number> {
            type Output = Result<Number>;

            fn $fn(self) -> Self::Output {
                self.into_iter().$fn().ok_or_else(|| {
                    anyhow!(concat!(
                        "cannot call ",
                        stringify!($name),
                        " with empty arguments",
                    ))
                })
            }
        }
    };
}

impl_expr_function_builtin!(impl Max(max) for self as Number -> Number);
impl_expr_function_builtin!(impl Min(min) for self as Number -> Number);

impl Value {
    // fn is_feature(&self) -> bool {
    //     matches!(self, Self::Feature(_))
    // }

    fn is_number(&self) -> bool {
        matches!(self, Self::Number(_))
    }

    // fn to_feature_opt(&self) -> Option<Feature> {
    //     match self {
    //         Self::Feature(value) => Some(*value),
    //         _ => None,
    //     }
    // }

    fn to_number_opt(&self) -> Option<Number> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

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

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Number(OrderedFloat<f64>);

impl Number {
    pub const fn new(value: f64) -> Self {
        Self(OrderedFloat(value))
    }

    pub fn from_i64(value: i64) -> Option<Self> {
        OrderedFloat::from_i64(value).map(Self)
    }

    pub fn from_u64(value: u64) -> Option<Self> {
        OrderedFloat::from_u64(value).map(Self)
    }

    pub fn from_f32(value: f32) -> Option<Self> {
        OrderedFloat::from_f32(value).map(Self)
    }

    pub const fn into_inner(self) -> f64 {
        self.0 .0
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum FunctionExpr {
    BuiltIn(BuiltInFunctionExpr),
    Custom(Literal),
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum BuiltInFunctionExpr {
    Max,
    Min,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct Literal(pub String);

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
