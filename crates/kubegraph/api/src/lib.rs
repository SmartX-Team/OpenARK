#[cfg(feature = "polars")]
extern crate polars as pl;

pub mod connector;
pub mod db;
pub mod frame;
pub mod graph;
pub mod ops;
pub mod query;
pub mod solver;
pub mod twin;
pub mod vm;

pub mod consts {
    pub const NAMESPACE: &str = "kubegraph";
}
