use std::ops::RangeFrom;

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

#[cfg(feature = "petgraph")]
pub(super) fn transform_petgraph_edges<M>(
    graph: &mut ::petgraph::stable_graph::StableDiGraph<super::GraphEntry, super::GraphEntry>,
    metadata: &M,
    name_map: super::GraphNameMap,
    lf: LazyFrame,
) -> ::anyhow::Result<()>
where
    M: super::GraphMetadataExt,
{
    let df = lf
        .collect()
        .map_err(|error| ::anyhow::anyhow!("failed to collect polars edges dataframe: {error}"))?;

    let columns = DataSeries::new(&df);
    for data in columns {
        let src = data.get_as_petgraph(&name_map, metadata.src())?;
        let sink = data.get_as_petgraph(&name_map, metadata.sink())?;
        graph.add_edge(src, sink, data);
    }
    Ok(())
}

#[cfg(feature = "petgraph")]
pub(super) fn transform_petgraph_nodes<M>(
    graph: &mut ::petgraph::stable_graph::StableDiGraph<super::GraphEntry, super::GraphEntry>,
    metadata: &M,
    lf: LazyFrame,
) -> ::anyhow::Result<super::GraphNameMap>
where
    M: super::GraphMetadataExt,
{
    let df = lf
        .collect()
        .map_err(|error| ::anyhow::anyhow!("failed to collect polars nodes dataframe: {error}"))?;

    let columns = DataSeries::new(&df);
    for data in columns {
        graph.add_node(data);
    }

    let name_key = metadata.name();
    let name = df.column(name_key).map_err(|error| {
        ::anyhow::anyhow!("failed to get polars nodes column ({name_key}): {error}")
    })?;

    Ok(name
        .iter()
        .filter_map(|name| match name {
            ::pl::datatypes::AnyValue::String(value) => Some(value.into()),
            ::pl::datatypes::AnyValue::StringOwned(value) => Some(value.into()),
            _ => None,
        })
        .enumerate()
        .map(|(index, name)| (name, index))
        .collect())
}

#[cfg(feature = "petgraph")]
struct DataSeries<'a> {
    columns: &'a [::pl::series::Series],
}

#[cfg(feature = "petgraph")]
impl<'a> DataSeries<'a> {
    fn new(df: &'a DataFrame) -> Self {
        let columns = df.get_columns();

        Self { columns }
    }
}

#[cfg(feature = "petgraph")]
impl<'a> IntoIterator for DataSeries<'a> {
    type Item = super::GraphEntry;

    type IntoIter = DataSeriesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        DataSeriesIter {
            indices: 0..,
            series: self,
        }
    }
}

#[cfg(feature = "petgraph")]
struct DataSeriesIter<'a> {
    indices: RangeFrom<usize>,
    series: DataSeries<'a>,
}

#[cfg(feature = "petgraph")]
impl<'a> Iterator for DataSeriesIter<'a> {
    type Item = super::GraphEntry;

    fn next(&mut self) -> Option<Self::Item> {
        use crate::vm::{Feature, Number};

        let index = self.indices.next()?;

        let mut entry = super::GraphEntry::default();
        for column in self.series.columns {
            let key = column.name().into();
            let value = match column.get(index).ok()? {
                ::pl::datatypes::AnyValue::Null => continue,
                ::pl::datatypes::AnyValue::Boolean(value) => {
                    super::GraphEntryValue::Feature(Feature::new(value))
                }
                ::pl::datatypes::AnyValue::UInt8(value) => {
                    super::GraphEntryValue::Number(Number::new(value.into()))
                }
                ::pl::datatypes::AnyValue::UInt16(value) => {
                    super::GraphEntryValue::Number(Number::new(value.into()))
                }
                ::pl::datatypes::AnyValue::UInt32(value) => {
                    super::GraphEntryValue::Number(Number::new(value.into()))
                }
                ::pl::datatypes::AnyValue::UInt64(value) => match Number::from_u64(value) {
                    Some(value) => super::GraphEntryValue::Number(value),
                    None => continue,
                },
                ::pl::datatypes::AnyValue::Int8(value) => {
                    super::GraphEntryValue::Number(Number::new(value.into()))
                }
                ::pl::datatypes::AnyValue::Int16(value) => {
                    super::GraphEntryValue::Number(Number::new(value.into()))
                }
                ::pl::datatypes::AnyValue::Int32(value) => {
                    super::GraphEntryValue::Number(Number::new(value.into()))
                }
                ::pl::datatypes::AnyValue::Int64(value) => match Number::from_i64(value) {
                    Some(value) => super::GraphEntryValue::Number(value),
                    None => continue,
                },
                ::pl::datatypes::AnyValue::Float32(value) => {
                    super::GraphEntryValue::Number(Number::new(value.into()))
                }
                ::pl::datatypes::AnyValue::Float64(value) => {
                    super::GraphEntryValue::Number(Number::new(value))
                }
                ::pl::datatypes::AnyValue::String(value) => {
                    super::GraphEntryValue::String(value.into())
                }
                ::pl::datatypes::AnyValue::StringOwned(value) => {
                    super::GraphEntryValue::String(value.into())
                }
                ::pl::datatypes::AnyValue::Date(_)
                | ::pl::datatypes::AnyValue::Datetime(_, _, _)
                | ::pl::datatypes::AnyValue::Duration(_, _)
                | ::pl::datatypes::AnyValue::Time(_)
                | ::pl::datatypes::AnyValue::List(_)
                | ::pl::datatypes::AnyValue::Struct(_, _, _)
                | ::pl::datatypes::AnyValue::StructOwned(_)
                | ::pl::datatypes::AnyValue::Binary(_)
                | ::pl::datatypes::AnyValue::BinaryOwned(_)
                | ::pl::datatypes::AnyValue::Decimal(_, _) => continue,
            };
            entry.others.insert(key, value);
        }
        Some(entry)
    }
}
