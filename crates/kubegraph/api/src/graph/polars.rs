use pl::{
    error::PolarsError,
    frame::DataFrame,
    lazy::frame::{IntoLazy, LazyFrame},
};

impl From<super::GraphData<LazyFrame>> for super::GraphData<super::LazyFrame> {
    fn from(graph: super::GraphData<LazyFrame>) -> Self {
        let super::GraphData { edges, nodes } = graph;
        Self {
            edges: super::LazyFrame::Polars(edges),
            nodes: super::LazyFrame::Polars(nodes),
        }
    }
}

impl From<super::GraphData<DataFrame>> for super::GraphData<LazyFrame> {
    fn from(graph: super::GraphData<DataFrame>) -> Self {
        let super::GraphData { edges, nodes } = graph;
        Self {
            edges: edges.lazy(),
            nodes: nodes.lazy(),
        }
    }
}

impl TryFrom<super::GraphData<LazyFrame>> for super::GraphData<DataFrame> {
    type Error = PolarsError;

    fn try_from(graph: super::GraphData<LazyFrame>) -> Result<Self, Self::Error> {
        let super::GraphData { edges, nodes } = graph;
        Ok(Self {
            edges: edges.collect()?,
            nodes: nodes.collect()?,
        })
    }
}
