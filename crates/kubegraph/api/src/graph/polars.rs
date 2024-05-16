use pl::{
    error::PolarsError,
    frame::DataFrame,
    lazy::frame::{IntoLazy, LazyFrame},
};

impl From<super::Graph<LazyFrame>> for super::Graph<super::LazyFrame> {
    fn from(graph: super::Graph<LazyFrame>) -> Self {
        let super::Graph { edges, nodes } = graph;
        Self {
            edges: super::LazyFrame::Polars(edges),
            nodes: super::LazyFrame::Polars(nodes),
        }
    }
}

impl From<super::Graph<DataFrame>> for super::Graph<LazyFrame> {
    fn from(graph: super::Graph<DataFrame>) -> Self {
        let super::Graph { edges, nodes } = graph;
        Self {
            edges: edges.lazy(),
            nodes: nodes.lazy(),
        }
    }
}

impl TryFrom<super::Graph<LazyFrame>> for super::Graph<DataFrame> {
    type Error = PolarsError;

    fn try_from(graph: super::Graph<LazyFrame>) -> Result<Self, Self::Error> {
        let super::Graph { edges, nodes } = graph;
        Ok(Self {
            edges: edges.collect()?,
            nodes: nodes.collect()?,
        })
    }
}
