use anyhow::{anyhow, bail, Result};
use pl::{
    datatypes::DataType,
    frame::DataFrame,
    lazy::{
        dsl,
        frame::{IntoLazy, LazyFrame},
    },
    series::Series,
};

use crate::graph::{Graph, IntoGraph};

impl From<DataFrame> for super::LazyFrame {
    fn from(df: DataFrame) -> Self {
        Self::Polars(df.lazy())
    }
}

impl From<LazyFrame> for super::LazyFrame {
    fn from(df: LazyFrame) -> Self {
        Self::Polars(df)
    }
}

impl IntoGraph<super::LazyFrame> for LazyFrame {
    fn try_into_graph(self) -> Result<Graph<super::LazyFrame>> {
        let nodes_src = self.clone().select([
            dsl::col("src").alias("name"),
            dsl::col(r"^src\..*$")
                .name()
                .map(|name| Ok(name["src.".len()..].into())),
        ]);
        let nodes_sink = self.clone().select([
            dsl::col("sink").alias("name"),
            dsl::col(r"^sink\..*$")
                .name()
                .map(|name| Ok(name["sink.".len()..].into())),
        ]);

        let args = dsl::UnionArgs::default();
        let nodes = dsl::concat_lf_diagonal([nodes_src, nodes_sink], args)
            .map_err(|error| anyhow!("failed to stack sink over src: {error}"))?
            .group_by([dsl::col("name")])
            .agg([dsl::all().sum()]);

        let edges = self.clone().select([
            dsl::col("src"),
            dsl::col("sink"),
            dsl::col(r"^link\..*$")
                .name()
                .map(|name| Ok(name["link.".len()..].into())),
        ]);

        Ok(Graph {
            edges: super::LazyFrame::Polars(edges),
            nodes: super::LazyFrame::Polars(nodes),
        })
    }
}

pub fn get_column(
    df: &DataFrame,
    kind: &str,
    key: &str,
    name: &str,
    dtype: Option<&DataType>,
) -> Result<Series> {
    let column = df
        .column(name)
        .map_err(|error| anyhow!("failed to get {kind} {key} column: {error}"))?;

    match dtype {
        Some(dtype) => column
            .cast(dtype)
            .map_err(|error| anyhow!("failed to cast {kind} {key} column as {dtype}: {error}")),
        None => Ok(column.clone()),
    }
}

pub fn find_indices(names: &Series, keys: &Series) -> Result<Option<Series>> {
    match names.dtype() {
        DataType::String => {
            let len_names = names
                .len()
                .try_into()
                .map_err(|error| anyhow!("failed to get node name length: {error}"))?;

            names
                .clone()
                .into_frame()
                .lazy()
                .with_column(dsl::lit(Series::from_iter(0..len_names).with_name("id")))
                .filter(dsl::col("name").eq(dsl::lit(keys.clone())))
                .select([dsl::col("id")])
                .collect()
                .map_err(|error| anyhow!("failed to find node name indices: {error}"))?
                .column("id")
                .map_err(|error| {
                    anyhow!("failed to get node id column; it should be a BUG: {error}")
                })
                .map(Clone::clone)
                .map(Some)
        }
        dtype if dtype.is_integer() => Ok(None),
        dtype => bail!("failed to use unknown type as node name: {dtype}"),
    }
}
