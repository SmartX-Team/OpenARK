#[cfg(feature = "polars")]
pub mod polars;

use std::ops::{Add, Div, Mul, Neg, Not, Sub};

use anyhow::Result;
#[cfg(feature = "polars")]
use pl::lazy::dsl;

use crate::{
    graph::{Graph, IntoGraph},
    ops::{And, Eq, Ge, Gt, Le, Lt, Ne, Or},
    vm::{Feature, Number},
};

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

impl LazyFrame {
    pub fn all(&self) -> LazySlice {
        match self {
            #[cfg(feature = "polars")]
            Self::Polars(_) => LazySlice::Polars(dsl::all()),
        }
    }

    pub fn get_column(&self, name: &str) -> LazySlice {
        match self {
            #[cfg(feature = "polars")]
            Self::Polars(_) => LazySlice::Polars(dsl::col(name)),
        }
    }

    pub fn insert_column(&mut self, name: &str, column: LazySlice) {
        match (self, column) {
            #[cfg(feature = "polars")]
            (Self::Polars(df), LazySlice::Polars(column)) => {
                *df = df.clone().with_column(column.alias(name));
            }
        }
    }

    pub fn apply_filter(&mut self, filter: LazySlice) {
        match (self, filter) {
            #[cfg(feature = "polars")]
            (Self::Polars(df), LazySlice::Polars(filter)) => *df = df.clone().filter(filter),
        }
    }

    pub fn fill_column_with_feature(&mut self, name: &str, value: Feature) {
        match self {
            #[cfg(feature = "polars")]
            Self::Polars(df) => {
                *df = df.clone().with_column(value.into_polars().alias(name));
            }
        }
    }

    pub fn fill_column_with_value(&mut self, name: &str, value: Number) {
        match self {
            #[cfg(feature = "polars")]
            Self::Polars(df) => {
                *df = df.clone().with_column(value.into_polars().alias(name));
            }
        }
    }

    #[cfg(feature = "polars")]
    pub fn try_into_polars(self) -> Result<::pl::lazy::frame::LazyFrame> {
        match self {
            Self::Polars(df) => Ok(df),
            _ => ::anyhow::bail!("failed to unwrap lazyframe as polars"),
        }
    }
}

impl IntoGraph<Self> for LazyFrame {
    fn try_into_graph(self) -> Result<Graph<Self>> {
        match self {
            #[cfg(feature = "polars")]
            LazyFrame::Polars(df) => df.try_into_graph(),
        }
    }
}

#[derive(Clone)]
pub enum LazySlice {
    #[cfg(feature = "polars")]
    Polars(dsl::Expr),
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
    fn into_polars(self) -> dsl::Expr
    where
        Self: Sized;
}

impl IntoLazySlice for Feature {
    #[cfg(feature = "polars")]
    fn into_polars(self) -> dsl::Expr {
        dsl::Expr::Literal(::pl::prelude::LiteralValue::Boolean(self.into_inner()))
    }
}

impl IntoLazySlice for Number {
    #[cfg(feature = "polars")]
    fn into_polars(self) -> dsl::Expr {
        dsl::Expr::Literal(::pl::prelude::LiteralValue::Float64(self.into_inner()))
    }
}
