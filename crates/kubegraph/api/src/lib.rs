#[cfg(feature = "df-polars")]
extern crate polars as pl;

pub mod component;
pub mod connector;
pub mod dependency;
pub mod frame;
pub mod function;
pub mod graph;
pub mod market;
pub mod ops;
pub mod problem;
pub mod query;
pub mod resource;
pub mod runner;
pub mod solver;
pub mod visualizer;
pub mod vm;

pub mod consts {
    pub const NAMESPACE: &str = "kubegraph";
}
