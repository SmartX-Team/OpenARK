#[cfg(feature = "df-polars")]
extern crate polars as pl;

pub mod analyzer;
pub mod connector;
pub mod frame;
pub mod function;
pub mod graph;
pub mod ops;
pub mod problem;
pub mod query;
pub mod resource;
pub mod runner;
pub mod solver;
pub mod vm;

pub mod consts {
    pub const NAMESPACE: &str = "kubegraph";
}
