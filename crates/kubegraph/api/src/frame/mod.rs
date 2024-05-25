#[cfg(feature = "df-polars")]
pub mod polars;

use std::ops::{Add, Div, Mul, Neg, Not, Sub};

use ::polars::datatypes::DataType;
use anyhow::{anyhow, bail, Result};
#[cfg(feature = "df-polars")]
use pl::lazy::dsl;
use serde::{Deserialize, Serialize};

use crate::{
    function::FunctionMetadata,
    graph::{GraphDataType, GraphMetadataPinnedExt, GraphScope},
    ops::{And, Eq, Ge, Gt, Le, Lt, Max, Min, Ne, Or},
    problem::ProblemSpec,
    vm::{Feature, Number},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DataFrame {
    Empty,
    #[cfg(feature = "df-polars")]
    Polars(::pl::frame::DataFrame),
}

impl DataFrame {
    pub fn drop_null_columns(self) -> Self {
        match self {
            Self::Empty => Self::Empty,
            #[cfg(feature = "df-polars")]
            Self::Polars(df) => {
                let null_columns: Vec<_> = df
                    .get_columns()
                    .iter()
                    .filter(|column| column.dtype() == &DataType::Null)
                    .map(|column| column.name().to_string())
                    .collect();

                let df_filtered = if !null_columns.is_empty() {
                    df.drop_many(&null_columns)
                } else {
                    df
                };
                Self::Polars(df_filtered)
            }
        }
    }

    pub fn lazy(self) -> LazyFrame {
        match self {
            Self::Empty => LazyFrame::Empty,
            #[cfg(feature = "df-polars")]
            Self::Polars(df) => LazyFrame::Polars(::pl::lazy::frame::IntoLazy::lazy(df)),
        }
    }
}

#[derive(Clone, Default)]
pub enum LazyFrame {
    #[default]
    Empty,
    #[cfg(feature = "df-polars")]
    Polars(::pl::lazy::frame::LazyFrame),
}

impl From<DataFrame> for LazyFrame {
    fn from(value: DataFrame) -> Self {
        match value {
            DataFrame::Empty => Self::Empty,
            #[cfg(feature = "df-polars")]
            DataFrame::Polars(df) => LazyFrame::Polars(::pl::lazy::frame::IntoLazy::lazy(df)),
        }
    }
}

impl LazyFrame {
    pub fn all(&self) -> Result<LazySlice> {
        match self {
            Self::Empty => bail!("cannot get all columns from empty lazyframe"),
            #[cfg(feature = "df-polars")]
            Self::Polars(_) => Ok(LazySlice::Polars(dsl::all())),
        }
    }

    pub fn cast<MF, MT>(self, ty: GraphDataType, from: &MF, to: &MT) -> Self
    where
        MF: GraphMetadataPinnedExt,
        MT: GraphMetadataPinnedExt,
    {
        match self {
            Self::Empty => Self::Empty,
            #[cfg(feature = "df-polars")]
            Self::Polars(df) => Self::Polars(self::polars::cast(df, ty, from, to)),
        }
    }

    pub async fn collect(self) -> Result<DataFrame> {
        match self {
            Self::Empty => Ok(DataFrame::Empty),
            #[cfg(feature = "df-polars")]
            Self::Polars(df) => df
                .collect()
                .map(DataFrame::Polars)
                .map_err(|error| ::anyhow::anyhow!("failed to collect polars dataframe: {error}")),
        }
    }

    pub fn concat(self, other: Self) -> Result<Self> {
        match (self, other) {
            (Self::Empty, Self::Empty) => Ok(Self::Empty),
            (Self::Empty, value) | (value, Self::Empty) => Ok(value),
            #[cfg(feature = "df-polars")]
            (Self::Polars(a), Self::Polars(b)) => self::polars::concat(a, b).map(Self::Polars),
        }
    }

    /// Create a fully-connected edges
    pub fn fabric<M>(&self, problem: &ProblemSpec<M>) -> Result<Self>
    where
        M: GraphMetadataPinnedExt,
    {
        let ProblemSpec {
            metadata,
            verbose: _,
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
                select_polars_edge_side(&nodes, metadata.name(), metadata.src())
                    .cross_join(select_polars_edge_side(
                        &nodes,
                        metadata.name(),
                        metadata.sink(),
                    ))
                    .with_column(
                        dsl::lit(ProblemSpec::<M>::MAX_CAPACITY).alias(metadata.capacity()),
                    ),
            )),
        }
    }

    pub fn get_column(&self, name: &str) -> Result<LazySlice> {
        match self {
            Self::Empty => bail!("cannot get column from empty lazyframe"),
            #[cfg(feature = "df-polars")]
            Self::Polars(_) => Ok(LazySlice::Polars(dsl::col(name))),
        }
    }

    pub fn alias(&mut self, key: &str, metadata: &FunctionMetadata) -> Result<()> {
        let FunctionMetadata {
            scope: GraphScope { namespace: _, name },
        } = metadata;

        match self {
            Self::Empty => bail!("cannot make an alias to empty lazyframe: {key:?}"),
            #[cfg(feature = "df-polars")]
            Self::Polars(df) => {
                *df = df.clone().with_column(dsl::lit(name.as_str()).alias(key));
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
            Self::Empty => bail!("cannot fill column with feature into empty lazyframe: {name:?}"),
            #[cfg(feature = "df-polars")]
            Self::Polars(df) => {
                *df = df.clone().with_column(value.into_polars().alias(name));
                Ok(())
            }
        }
    }

    pub fn fill_column_with_value(&mut self, name: &str, value: Number) -> Result<()> {
        match self {
            Self::Empty => bail!("cannot fill column with name into empty lazyframe: {name:?}"),
            #[cfg(feature = "df-polars")]
            Self::Polars(df) => {
                *df = df.clone().with_column(value.into_polars().alias(name));
                Ok(())
            }
        }
    }

    pub fn insert_column(&mut self, name: &str, column: LazySlice) -> Result<()> {
        match (self, column) {
            (Self::Empty, _) => bail!("cannot fill column into empty lazyframe: {name:?}"),
            #[cfg(feature = "df-polars")]
            (Self::Polars(df), LazySlice::Polars(column)) => {
                *df = df.clone().with_column(column.alias(name));
                Ok(())
            }
        }
    }

    #[cfg(feature = "df-polars")]
    pub fn try_into_polars(self) -> Result<::pl::lazy::frame::LazyFrame> {
        match self {
            Self::Empty => Ok(::pl::lazy::frame::LazyFrame::default()),
            Self::Polars(df) => Ok(df),
        }
    }
}

#[derive(Clone)]
pub enum LazySliceOrScalar<T> {
    LazySlice(LazySlice),
    Scalar(T),
}

macro_rules! impl_expr_function_builtin {
    ( impl $ty:ident ( $fn:ident ) for Vec<LazySliceOrScalar< $scalar:ty >> {
        polars: {
            op: $fn_polars:ident,
            acc: $acc_polars:expr,
        },
    } ) => {
        impl $ty for Vec<LazySliceOrScalar<$scalar>> {
            type Output = Result<LazySliceOrScalar<$scalar>>;

            fn $fn(mut self) -> Self::Output {
                let mut acc = self.pop().ok_or_else(|| {
                    anyhow!(concat!(
                        "cannot call ",
                        stringify!($name),
                        " with empty arguments",
                    ))
                })?;

                while let Some(arg) = self.pop() {
                    acc = match (acc, arg) {
                        (LazySliceOrScalar::LazySlice(_), LazySliceOrScalar::LazySlice(_)) => {
                            bail!(concat!(
                                "cannot call ",
                                stringify!($name),
                                " with multiple slices",
                            ))
                        }
                        #[cfg(feature = "df-polars")]
                        (
                            LazySliceOrScalar::LazySlice(LazySlice::Polars(slice)),
                            LazySliceOrScalar::Scalar(b),
                        )
                        | (
                            LazySliceOrScalar::Scalar(b),
                            LazySliceOrScalar::LazySlice(LazySlice::Polars(slice)),
                        ) => {
                            let a = slice.$fn_polars();
                            let b = dsl::lit(b);

                            let acc = $acc_polars(a, b);
                            LazySliceOrScalar::LazySlice(LazySlice::Polars(acc))
                        }
                        (LazySliceOrScalar::Scalar(a), LazySliceOrScalar::Scalar(b)) => {
                            LazySliceOrScalar::Scalar(a.$fn(b))
                        }
                    };
                }

                Ok(acc)
            }
        }
    };
}

impl_expr_function_builtin!(impl Max(max) for Vec<LazySliceOrScalar<Number>> {
    polars: {
        op: max,
        acc: |a: dsl::Expr, b: dsl::Expr| dsl::when(a.clone().gt_eq(b.clone())).then(a).otherwise(b),
    },
});
impl_expr_function_builtin!(impl Min(min) for Vec<LazySliceOrScalar<Number>> {
    polars: {
        op: min,
        acc: |a: dsl::Expr, b: dsl::Expr| dsl::when(a.clone().lt_eq(b.clone())).then(a).otherwise(b),
    },
});

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
        dsl::Expr::Literal(::pl::prelude::LiteralValue::Int64(
            self.into_inner().round() as i64,
        ))
    }
}
