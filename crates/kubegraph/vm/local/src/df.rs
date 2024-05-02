use std::ops::{Add, Div, Mul, Neg, Not, Sub};

use anyhow::{anyhow, Result};
use kubegraph_api::{
    graph::Graph,
    vm::{And, Eq, Feature, Ge, Gt, Le, Lt, Ne, Number, Or},
};
#[cfg(feature = "polars")]
use pl::lazy::frame::IntoLazy;

pub trait IntoLazyFrame
where
    Self: Into<LazyFrame>,
{
}

impl<T> IntoLazyFrame for T where T: Into<LazyFrame> {}

#[derive(Clone)]
pub enum LazyFrame {
    #[cfg(feature = "polars")]
    Polars(::pl::lazy::frame::LazyFrame),
}

#[cfg(feature = "polars")]
impl From<::pl::frame::DataFrame> for LazyFrame {
    fn from(df: ::pl::frame::DataFrame) -> Self {
        Self::Polars(df.lazy())
    }
}

#[cfg(feature = "polars")]
impl From<::pl::lazy::frame::LazyFrame> for LazyFrame {
    fn from(df: ::pl::lazy::frame::LazyFrame) -> Self {
        Self::Polars(df)
    }
}

impl LazyFrame {
    pub(crate) fn all(&self) -> LazySlice {
        match self {
            #[cfg(feature = "polars")]
            Self::Polars(_) => LazySlice::Polars(::pl::lazy::dsl::all()),
        }
    }

    pub(crate) fn get_column(&self, name: &str) -> LazySlice {
        match self {
            #[cfg(feature = "polars")]
            Self::Polars(_) => LazySlice::Polars(::pl::lazy::dsl::col(name)),
        }
    }

    pub(crate) fn insert_column(&mut self, name: &str, column: LazySlice) {
        match (self, column) {
            #[cfg(feature = "polars")]
            (Self::Polars(df), LazySlice::Polars(column)) => {
                *df = df.clone().with_column(column.alias(name));
            }
        }
    }

    pub(crate) fn apply_filter(&mut self, filter: LazySlice) {
        match (self, filter) {
            #[cfg(feature = "polars")]
            (Self::Polars(df), LazySlice::Polars(filter)) => *df = df.clone().filter(filter),
        }
    }

    pub(crate) fn fill_column_with_feature(&mut self, name: &str, value: Feature) {
        match self {
            #[cfg(feature = "polars")]
            Self::Polars(df) => {
                *df = df.clone().with_column(value.into_polars().alias(name));
            }
        }
    }

    pub(crate) fn fill_column_with_value(&mut self, name: &str, value: Number) {
        match self {
            #[cfg(feature = "polars")]
            Self::Polars(df) => {
                *df = df.clone().with_column(value.into_polars().alias(name));
            }
        }
    }

    pub(crate) fn try_into_graph(self) -> Result<Graph<Self>> {
        match self {
            #[cfg(feature = "polars")]
            LazyFrame::Polars(graph_df) => {
                let nodes_src = graph_df.clone().select(&[
                    ::pl::lazy::dsl::col("src").alias("name"),
                    ::pl::lazy::dsl::col(r"^src\..*$")
                        .name()
                        .map(|name| Ok(name["src.".len()..].into())),
                ]);
                let nodes_sink = graph_df.clone().select(&[
                    ::pl::lazy::dsl::col("sink").alias("name"),
                    ::pl::lazy::dsl::col(r"^sink\..*$")
                        .name()
                        .map(|name| Ok(name["sink.".len()..].into())),
                ]);

                let args = ::pl::lazy::prelude::UnionArgs::default();
                let nodes = ::pl::lazy::prelude::concat_lf_diagonal(&[nodes_src, nodes_sink], args)
                    .map_err(|error| anyhow!("failed to stack sink over src: {error}"))?;

                let edges = graph_df.clone().select(&[
                    ::pl::lazy::dsl::col("src"),
                    ::pl::lazy::dsl::col("sink"),
                    ::pl::lazy::dsl::col(r"^link\..*$")
                        .name()
                        .map(|name| Ok(name["link.".len()..].into())),
                ]);

                Ok(Graph {
                    edges: LazyFrame::Polars(edges),
                    nodes: LazyFrame::Polars(nodes),
                })
            }
        }
    }
}

#[derive(Clone)]
pub enum LazySlice {
    #[cfg(feature = "polars")]
    Polars(::pl::lazy::dsl::Expr),
}

macro_rules! impl_expr_unary {
    ( impl $ty:ident ( $fn:ident ) for LazySlice {
        polars: $fn_polars:ident,
    } ) => {
        impl $ty for LazySlice {
            type Output = Self;

            fn $fn(self) -> Self::Output {
                match self {
                    #[cfg(feature = "polars")]
                    Self::Polars(src) => Self::Polars(src.$fn_polars()),
                }
            }
        }
    };
}

impl_expr_unary!(impl Neg(neg) for LazySlice {
    polars: neg,
});
impl_expr_unary!(impl Not(not) for LazySlice {
    polars: not,
});

macro_rules! impl_expr_binary {
    ( impl $ty:ident ( $fn:ident ) for $target:ident {
        polars: $fn_polars:ident,
    } ) => {
        impl $ty for LazySlice {
            type Output = Self;

            fn $fn(self, rhs: Self) -> Self::Output {
                match (self, rhs) {
                    #[cfg(feature = "polars")]
                    (Self::Polars(lhs), Self::Polars(rhs)) => Self::Polars(lhs.$fn_polars(rhs)),
                }
            }
        }

        impl $ty<$target> for LazySlice {
            type Output = Self;

            fn $fn(self, rhs: $target) -> Self::Output {
                match self {
                    #[cfg(feature = "polars")]
                    Self::Polars(lhs) => {
                        let rhs = rhs.into_polars();
                        Self::Polars(lhs.$fn_polars(rhs))
                    }
                }
            }
        }

        impl $ty<LazySlice> for $target {
            type Output = LazySlice;

            fn $fn(self, rhs: LazySlice) -> Self::Output {
                match rhs {
                    #[cfg(feature = "polars")]
                    LazySlice::Polars(rhs) => {
                        let lhs = self.into_polars();
                        LazySlice::Polars(lhs.$fn_polars(rhs))
                    }
                }
            }
        }
    };
}

impl_expr_binary!(impl Add(add) for Number {
    polars: add,
});
impl_expr_binary!(impl Sub(sub) for Number {
    polars: sub,
});
impl_expr_binary!(impl Mul(mul) for Number {
    polars: mul,
});
impl_expr_binary!(impl Div(div) for Number {
    polars: div,
});
impl_expr_binary!(impl Eq(eq) for Number {
    polars: eq,
});
impl_expr_binary!(impl Ne(ne) for Number {
    polars: neq,
});
impl_expr_binary!(impl Ge(ge) for Number {
    polars: gt_eq,
});
impl_expr_binary!(impl Gt(gt) for Number {
    polars: gt,
});
impl_expr_binary!(impl Le(le) for Number {
    polars: lt_eq,
});
impl_expr_binary!(impl Lt(lt) for Number {
    polars: lt,
});
impl_expr_binary!(impl And(and) for Feature {
    polars: and,
});
impl_expr_binary!(impl Or(or) for Feature {
    polars: or,
});

pub trait IntoLazySlice {
    fn into_lazy_slice(self, df: &LazyFrame) -> LazySlice
    where
        Self: Sized,
    {
        match df {
            #[cfg(feature = "polars")]
            LazyFrame::Polars(_) => LazySlice::Polars(self.into_polars()),
        }
    }

    #[cfg(feature = "polars")]
    fn into_polars(self) -> ::pl::lazy::dsl::Expr
    where
        Self: Sized;
}

impl IntoLazySlice for Feature {
    #[cfg(feature = "polars")]
    fn into_polars(self) -> ::pl::lazy::dsl::Expr {
        ::pl::lazy::dsl::Expr::Literal(::pl::prelude::LiteralValue::Boolean(self.into_inner()))
    }
}

impl IntoLazySlice for Number {
    #[cfg(feature = "polars")]
    fn into_polars(self) -> ::pl::lazy::dsl::Expr {
        ::pl::lazy::dsl::Expr::Literal(::pl::prelude::LiteralValue::Float64(self.into_inner()))
    }
}
