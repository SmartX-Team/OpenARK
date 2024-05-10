#[cfg(feature = "df-polars")]
pub mod polars;

use std::ops::{Add, Div, Mul, Neg, Not, Sub};

use anyhow::{bail, Result};
#[cfg(feature = "df-polars")]
use pl::lazy::dsl;

use crate::{
    func::FunctionMetadata,
    ops::{And, Eq, Ge, Gt, Le, Lt, Ne, Or},
    solver::{Problem, ProblemMetadata},
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
    Empty,
    #[cfg(feature = "df-polars")]
    Polars(::pl::lazy::frame::LazyFrame),
}

impl LazyFrame {
    pub fn all(&self) -> Result<LazySlice> {
        match self {
            Self::Empty => bail!("cannot get all columns from empty lazyframe"),
            #[cfg(feature = "df-polars")]
            Self::Polars(_) => Ok(LazySlice::Polars(dsl::all())),
        }
    }

    pub fn get_column(&self, name: &str) -> Result<LazySlice> {
        match self {
            Self::Empty => bail!("cannot get column from empty lazyframe"),
            #[cfg(feature = "df-polars")]
            Self::Polars(_) => Ok(LazySlice::Polars(dsl::col(name))),
        }
    }

    /// Create a fully-connected edges
    pub fn fabric<T>(&self, problem: &Problem<T>) -> Result<Self>
    where
        T: AsRef<str>,
    {
        let Problem {
            metadata:
                ProblemMetadata {
                    flow: _,
                    function: _,
                    name,
                    sink,
                    src,
                    verbose: _,
                },
            capacity,
            cost: _,
            supply: _,
        } = problem;

        #[cfg(feature = "df-polars")]
        fn select_polars_edge_side(
            nodes: &::pl::lazy::frame::LazyFrame,
            name: &str,
            side: &str,
        ) -> ::pl::lazy::frame::LazyFrame {
            nodes.clone().select([
                dsl::col(name).alias(side),
                dsl::all()
                    .exclude([format!(r"^{name}$")])
                    .name()
                    .prefix(&format!("{side}.")),
            ])
        }

        match self {
            Self::Empty => bail!("cannot get fabric from empty lazyframe"),
            #[cfg(feature = "df-polars")]
            Self::Polars(nodes) => Ok(Self::Polars(
                select_polars_edge_side(&nodes, name, src)
                    .cross_join(select_polars_edge_side(&nodes, name, sink))
                    .with_column(dsl::lit(ProblemMetadata::MAX_CAPACITY).alias(capacity.as_ref())),
            )),
        }
    }

    pub fn alias(&mut self, key: &str, metadata: &FunctionMetadata) -> Result<()> {
        let FunctionMetadata { name } = metadata;

        match self {
            Self::Empty => bail!("cannot make an alias to empty lazyframe"),
            #[cfg(feature = "df-polars")]
            Self::Polars(df) => {
                *df = df.clone().with_column(dsl::lit(name.as_str()).alias(key));
                Ok(())
            }
        }
    }

    pub fn insert_column(&mut self, name: &str, column: LazySlice) -> Result<()> {
        match (self, column) {
            (Self::Empty, _) => bail!("cannot fill column into empty lazyframe"),
            #[cfg(feature = "df-polars")]
            (Self::Polars(df), LazySlice::Polars(column)) => {
                *df = df.clone().with_column(column.alias(name));
                Ok(())
            }
        }
    }

    pub fn apply_filter(&mut self, filter: LazySlice) -> Result<()> {
        match (self, filter) {
            (Self::Empty, _) => bail!("cannot apply filter into empty lazyframe"),
            #[cfg(feature = "df-polars")]
            (Self::Polars(df), LazySlice::Polars(filter)) => {
                *df = df.clone().filter(filter);
                Ok(())
            }
        }
    }

    pub fn fill_column_with_feature(&mut self, name: &str, value: Feature) -> Result<()> {
        match self {
            Self::Empty => bail!("cannot fill column with feature into empty lazyframe"),
            #[cfg(feature = "df-polars")]
            Self::Polars(df) => {
                *df = df.clone().with_column(value.into_polars().alias(name));
                Ok(())
            }
        }
    }

    pub fn fill_column_with_value(&mut self, name: &str, value: Number) -> Result<()> {
        match self {
            Self::Empty => bail!("cannot fill column with name into empty lazyframe"),
            #[cfg(feature = "df-polars")]
            Self::Polars(df) => {
                *df = df.clone().with_column(value.into_polars().alias(name));
                Ok(())
            }
        }
    }

    #[cfg(feature = "df-polars")]
    pub fn try_into_polars(self) -> Result<::pl::lazy::frame::LazyFrame> {
        match self {
            Self::Polars(df) => Ok(df),
            _ => ::anyhow::bail!("failed to unwrap lazyframe as polars"),
        }
    }
}

#[derive(Clone)]
pub enum LazySlice {
    #[cfg(feature = "df-polars")]
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
                    #[cfg(feature = "df-polars")]
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
                    #[cfg(feature = "df-polars")]
                    (Self::Polars(lhs), Self::Polars(rhs)) => Self::Polars(lhs.$fn_polars(rhs)),
                }
            }
        }

        impl $ty<$target> for LazySlice {
            type Output = Self;

            fn $fn(self, rhs: $target) -> Self::Output {
                match self {
                    #[cfg(feature = "df-polars")]
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
                    #[cfg(feature = "df-polars")]
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
    fn try_into_lazy_slice(self, df: &LazyFrame) -> Result<LazySlice>
    where
        Self: Sized,
    {
        match df {
            LazyFrame::Empty => bail!("cannot get slice from empty lazyframe"),
            #[cfg(feature = "df-polars")]
            LazyFrame::Polars(_) => Ok(LazySlice::Polars(self.into_polars())),
        }
    }

    #[cfg(feature = "df-polars")]
    fn into_polars(self) -> dsl::Expr
    where
        Self: Sized;
}

impl IntoLazySlice for Feature {
    #[cfg(feature = "df-polars")]
    fn into_polars(self) -> dsl::Expr {
        dsl::Expr::Literal(::pl::prelude::LiteralValue::Boolean(self.into_inner()))
    }
}

impl IntoLazySlice for Number {
    #[cfg(feature = "df-polars")]
    fn into_polars(self) -> dsl::Expr {
        dsl::Expr::Literal(::pl::prelude::LiteralValue::Float64(self.into_inner()))
    }
}
