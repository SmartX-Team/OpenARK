use std::ops::{Add, Div, Mul, Neg, Not, Sub};

use kubegraph_api::vm::{And, Eq, Feature, Ge, Gt, Le, Lt, Ne, Number, Or};
#[cfg(feature = "polars")]
use pl::lazy::frame::IntoLazy;

pub trait IntoDataFrame
where
    Self: Into<DataFrame>,
{
}

impl<T> IntoDataFrame for T where T: Into<DataFrame> {}

#[derive(Clone)]
pub enum DataFrame {
    #[cfg(feature = "polars")]
    PolarsLazy(::pl::lazy::frame::LazyFrame),
}

#[cfg(feature = "polars")]
impl From<::pl::frame::DataFrame> for DataFrame {
    fn from(df: ::pl::frame::DataFrame) -> Self {
        Self::PolarsLazy(df.lazy())
    }
}

#[cfg(feature = "polars")]
impl From<::pl::lazy::frame::LazyFrame> for DataFrame {
    fn from(df: ::pl::lazy::frame::LazyFrame) -> Self {
        Self::PolarsLazy(df)
    }
}

impl DataFrame {
    pub fn get_column(&self, name: &str) -> DataSlice {
        match self {
            #[cfg(feature = "polars")]
            Self::PolarsLazy(_) => DataSlice::PolarsLazy(::pl::lazy::dsl::col(name)),
        }
    }

    pub fn insert_column(&mut self, name: &str, column: DataSlice) {
        match (self, column) {
            #[cfg(feature = "polars")]
            (Self::PolarsLazy(df), DataSlice::PolarsLazy(column)) => {
                *df = df.clone().with_column(column.alias(name));
            }
        }
    }

    pub fn fill_column_with_feature(&mut self, name: &str, value: Feature) {
        match self {
            #[cfg(feature = "polars")]
            Self::PolarsLazy(df) => {
                *df = df.clone().with_column(value.into_lit().alias(name));
            }
        }
    }

    pub fn fill_column_with_value(&mut self, name: &str, value: Number) {
        match self {
            #[cfg(feature = "polars")]
            Self::PolarsLazy(df) => {
                *df = df.clone().with_column(value.into_lit().alias(name));
            }
        }
    }
}

#[derive(Clone)]
pub enum DataSlice {
    #[cfg(feature = "polars")]
    PolarsLazy(::pl::lazy::dsl::Expr),
}

macro_rules! impl_expr_unary {
    ( impl $ty:ident ( $fn:ident ) for DataSlice {
        polars: $fn_polars:ident,
    } ) => {
        impl $ty for DataSlice {
            type Output = Self;

            fn $fn(self) -> Self::Output {
                match self {
                    #[cfg(feature = "polars")]
                    Self::PolarsLazy(src) => Self::PolarsLazy(src.$fn_polars()),
                }
            }
        }
    };
}

impl_expr_unary!(impl Neg(neg) for DataSlice {
    polars: neg,
});
impl_expr_unary!(impl Not(not) for DataSlice {
    polars: not,
});

macro_rules! impl_expr_binary {
    ( impl $ty:ident ( $fn:ident ) for $target:ident {
        polars: $fn_polars:ident,
    } ) => {
        impl $ty for DataSlice {
            type Output = Self;

            fn $fn(self, rhs: Self) -> Self::Output {
                match (self, rhs) {
                    #[cfg(feature = "polars")]
                    (Self::PolarsLazy(lhs), Self::PolarsLazy(rhs)) => {
                        Self::PolarsLazy(lhs.$fn_polars(rhs))
                    }
                }
            }
        }

        impl $ty<$target> for DataSlice {
            type Output = Self;

            fn $fn(self, rhs: $target) -> Self::Output {
                match self {
                    #[cfg(feature = "polars")]
                    Self::PolarsLazy(lhs) => {
                        let rhs = rhs.into_lit();
                        Self::PolarsLazy(lhs.$fn_polars(rhs))
                    }
                }
            }
        }

        impl $ty<DataSlice> for $target {
            type Output = DataSlice;

            fn $fn(self, rhs: DataSlice) -> Self::Output {
                match rhs {
                    #[cfg(feature = "polars")]
                    DataSlice::PolarsLazy(rhs) => {
                        let lhs = self.into_lit();
                        DataSlice::PolarsLazy(lhs.$fn_polars(rhs))
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

#[cfg(feature = "polars")]
trait IntoLiteral {
    fn into_lit(self) -> ::pl::lazy::dsl::Expr;
}

#[cfg(feature = "polars")]
impl IntoLiteral for Feature {
    fn into_lit(self) -> pl::lazy::dsl::Expr {
        ::pl::lazy::dsl::Expr::Literal(::pl::prelude::LiteralValue::Boolean(self.into_inner()))
    }
}

#[cfg(feature = "polars")]
impl IntoLiteral for Number {
    fn into_lit(self) -> pl::lazy::dsl::Expr {
        ::pl::lazy::dsl::Expr::Literal(::pl::prelude::LiteralValue::Float64(self.into_inner()))
    }
}
