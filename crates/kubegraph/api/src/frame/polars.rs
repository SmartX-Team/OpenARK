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

use crate::graph::{GraphEdges, NetworkEdge, NetworkNode};

macro_rules! create_df_from_array {
    ( $( impl IntoLazyFrame for $ty:ty {
        $(
            $col_name:literal => $col_cast:expr ,
        )*
    } )* ) => { $(
        impl super::IntoLazyFrame<LazyFrame> for $ty {
            fn try_into_lazy_frame(self) -> Result<LazyFrame> {
                DataFrame::new(vec![ $(
                    Series::from_iter(self.iter().map($col_cast))
                        .with_name($col_name),
                )* ])
                .map(IntoLazy::lazy)
                .map_err(Into::into)
            }
        }
    )* };
}

create_df_from_array! {
    impl IntoLazyFrame for Vec<NetworkEdge> {
        "namespace" => |item| item.key.link.namespace.as_str(),
        "name" => |item| item.key.link.name.as_str(),
        "src.namespace" => |item| item.key.src.namespace.as_str(),
        "src" => |item| item.key.src.name.as_str(),
        "sink.namespace" => |item| item.key.sink.namespace.as_str(),
        "sink" => |item| item.key.sink.name.as_str(),
        "interval_ms" => |item| item.key.interval_ms,
        "capacity" => |item| item.value.capacity,
        "unit_cost" => |item| item.value.unit_cost,
    }

    impl IntoLazyFrame for Vec<NetworkNode> {
        "namespace" => |item| item.key.namespace.as_str(),
        "name" => |item| item.key.name.as_str(),
        "capacity" => |item| item.value.capacity,
        "supply" => |item| item.value.supply,
        "unit_cost" => |item| item.value.unit_cost,
    }
}

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

impl FromIterator<GraphEdges<LazyFrame>> for GraphEdges<super::LazyFrame> {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = GraphEdges<LazyFrame>>,
    {
        let args = dsl::UnionArgs {
            to_supertypes: true,
            ..Default::default()
        };
        let inputs: Vec<_> = iter.into_iter().map(|GraphEdges(edges)| edges).collect();
        dsl::concat_lf_diagonal(inputs, args)
            .map(super::LazyFrame::Polars)
            .map(Self)
            .unwrap_or(GraphEdges(super::LazyFrame::Empty))
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

pub fn find_index(key_name: &str, names: &Series, query: &str) -> Result<i32> {
    let len_names = names
        .len()
        .try_into()
        .map_err(|error| anyhow!("failed to get node name length: {error}"))?;

    let key_id = format!("{key_name}.id");
    names
        .clone()
        .into_frame()
        .lazy()
        .with_column(dsl::lit(Series::from_iter(0..len_names).with_name(&key_id)))
        .filter(dsl::col(key_name).eq(dsl::lit(query).cast(names.dtype().clone())))
        .select([dsl::col(&key_id)])
        .first()
        .collect()
        .map_err(|error| anyhow!("failed to find node name index: {error}"))?
        .column(&key_id)
        .map_err(|error| anyhow!("failed to get node id column; it should be a BUG: {error}"))
        .and_then(|column| column.get(0).map_err(|_| anyhow!("no such name: {query}")))
        .and_then(|value| {
            value.try_extract().map_err(|error| {
                anyhow!("failed to convert id column to usize; it should be a BUG: {error}")
            })
        })
}

pub fn find_indices(key_name: &str, names: &Series, keys: &Series) -> Result<Option<Series>> {
    match names.dtype() {
        DataType::String => {
            let len_names = names
                .len()
                .try_into()
                .map_err(|error| anyhow!("failed to get node name length: {error}"))?;

            let key_id = format!("{key_name}.id");
            names
                .clone()
                .into_frame()
                .lazy()
                .with_column(dsl::lit(Series::from_iter(0..len_names).with_name(&key_id)))
                .filter(dsl::col(key_name).is_in(dsl::lit(keys.clone())))
                .select([dsl::col(&key_id)])
                .collect()
                .map_err(|error| anyhow!("failed to find node name indices: {error}"))?
                .column(&key_id)
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
