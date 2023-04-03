use std::{
    fmt,
    io::{self, Write},
    marker::PhantomData,
};

use bytes::{BufMut, Bytes, BytesMut};
use dash_api::{
    model::{ModelFieldKindNativeSpec, ModelFieldNativeSpec},
    serde_json::Value,
};
use influxdb2::{
    api::write::TimestampPrecision,
    models::{Query, WriteDataPoint},
    FromMap,
};
use influxdb2_derive::{FromDataPoint, WriteDataPoint};
use influxdb2_structmap::GenericMap;
use ipis::{
    core::{
        anyhow::Result,
        chrono::{DateTime, Duration, FixedOffset, Utc},
    },
    env,
};
use paste::paste;
use serde::{Deserialize, Serialize};

pub struct Client {
    api: ::influxdb2::Client,
    bucket: String,
    namespace: String,
}

impl Client {
    pub fn try_default() -> Result<Self> {
        let host: String =
            env::infer("INFLUXDB_HOST").unwrap_or_else(|_| "http://localhost:8086".into());
        let org: String = env::infer("INFLUXDB_ORG_ID")?;
        let token: String = env::infer("INFLUXDB_TOKEN")?;
        let bucket: String = env::infer("INFLUXDB_BUCKET")?;
        let namespace: String = env::infer("INFLUXDB_NAMESPACE")?;

        Ok(Self {
            api: ::influxdb2::Client::new(host, org, token),
            bucket,
            namespace,
        })
    }

    pub async fn write_json(
        &self,
        fields: impl IntoIterator<Item = &ModelFieldNativeSpec>,
        value: &Value,
    ) -> Result<()> {
        let time = Utc::now().timestamp_nanos();
        let precision = TimestampPrecision::Nanoseconds;

        let body = fields
            .into_iter()
            .filter_map(move |field| Data::from_json(self, field, value, time))
            .flatten()
            .try_fold(BytesMut::new(), move |mut buf, point| {
                let mut w = (&mut buf).writer();
                point.write_data_point_to(&mut w)?;
                w.flush()?;
                Ok::<_, io::Error>(buf)
            })
            .map(Bytes::from)
            .map(::reqwest::Body::from)?;

        self.api
            .write_line_protocol_with_precision(&self.api.org, &self.bucket, body, precision)
            .await
            .map_err(Into::into)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueryBuilder<'a, T> {
    _output: PhantomData<T>,
    options: QueryOptions<'a>,
}

impl<'a, T> QueryBuilder<'a, T> {
    pub const fn start(mut self, duration: Duration) -> Self {
        self.options.range.start = Some(duration);
        self
    }

    pub const fn end(mut self, duration: Duration) -> Self {
        self.options.range.end = Some(duration);
        self
    }

    pub const fn name(mut self, value: &'a str) -> Self {
        self.options.filter_name = Some(value);
        self
    }

    pub const fn user(mut self, value: &'a str) -> Self {
        self.options.filter_user = Some(value);
        self
    }

    pub const fn group(mut self, columns: &'a [&'a str]) -> Self {
        self.options.group = Some(columns);
        self
    }

    pub async fn all(&self, client: &Client) -> Result<Vec<T>>
    where
        T: FromMap,
    {
        self.execute(client, None).await
    }

    pub async fn last(&self, client: &Client) -> Result<Vec<T>>
    where
        T: FromMap,
    {
        self.execute(client, Some("|>last()")).await
    }

    async fn execute(&self, client: &Client, collect_raw: Option<&str>) -> Result<Vec<T>>
    where
        T: FromMap,
    {
        let query = QueryFull {
            bucket: &client.bucket,
            collect_raw,
            options: QueryOptions {
                filter_namespace: Some(&client.namespace),
                ..self.options
            },
        };
        let query = Query::new(query.to_string());

        client.api.query(Some(query)).await.map_err(Into::into)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct QueryFull<'a> {
    bucket: &'a str,
    collect_raw: Option<&'a str>,
    options: QueryOptions<'a>,
}

impl<'a> fmt::Display for QueryFull<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bucket = self.bucket;
        let options = self.options;
        write!(f, "from(bucket: \"{bucket}\"){options}")?;

        match self.collect_raw {
            Some(collect_raw) => collect_raw.fmt(f),
            None => Ok(()),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct QueryOptions<'a> {
    filter_measurement: Option<&'a str>,
    filter_namespace: Option<&'a str>,
    filter_name: Option<&'a str>,
    filter_user: Option<&'a str>,
    group: Option<&'a [&'a str]>,
    range: QueryRange,
}

impl<'a> fmt::Display for QueryOptions<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn write_filter<T>(
            f: &mut fmt::Formatter<'_>,
            key: &'static str,
            value: Option<T>,
        ) -> fmt::Result
        where
            T: fmt::Display,
        {
            match value {
                Some(value) => write!(f, "|>filter(fn: (r) => r.{key} == \"{value}\")"),
                None => Ok(()),
            }
        }

        // range
        self.range.fmt(f)?;

        // group
        if let Some(group) = self.group {
            write!(f, "|>group(columns: [")?;
            for key in group {
                write!(f, "\"{key}\",")?;
            }
            write!(f, "])")?;
        }

        // filters
        write_filter(f, "_measurement", self.filter_measurement)?;
        write_filter(f, "namespace", self.filter_namespace)?;
        write_filter(f, "name", self.filter_name)?;
        write_filter(f, "user", self.filter_user)?;

        // finished
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct QueryRange {
    start: Option<Duration>,
    end: Option<Duration>,
}

impl fmt::Display for QueryRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.start, self.end) {
            (Some(start), Some(end)) => write!(
                f,
                "|>range(start: -{}s, end: -{}s)",
                start.num_seconds(),
                end.num_seconds(),
            ),
            (Some(start), None) => write!(f, "|>range(start: -{}s)", start.num_seconds()),
            (None, Some(end)) => write!(f, "|>range(end: -{}s)", end.num_seconds()),
            (None, None) => Ok(()),
        }
    }
}

macro_rules! define_data {
    ( $( $name:ident $( => query as $query:ty )? => write as $write:ty $( => cast as $cast:expr )? , )* ) => {
        paste! {
            $(
                #[derive(
                    Clone, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize, FromDataPoint,
                )]
                pub struct [< $name Query >] {
                    time: DateTime<FixedOffset>,
                    namespace: String,
                    name: String,
                    user: String,
                    $(
                        value: $query,
                    )?
                }

                impl [< $name Query >] {
                    const fn measurement() -> &'static str {
                        concat!( stringify!($name), "Data" )
                    }

                    pub const fn builder<'a>() -> QueryBuilder<'a, Self>
                    where
                        Self: Sized,
                    {
                        QueryBuilder {
                            _output: PhantomData,
                            options: QueryOptions {
                                filter_measurement: Some(Self::measurement()),
                                filter_namespace: None,
                                filter_name: None,
                                filter_user: None,
                                group: None,
                                range: QueryRange {
                                    start: None,
                                    end: None,
                                },
                            },
                        }
                    }
                }

                #[derive(
                    Clone, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize, WriteDataPoint,
                )]
                struct [< $name Data >]<'a> {
                    #[influxdb(timestamp)]
                    time: i64, // in nanoseconds
                    #[influxdb(tag)]
                    namespace: &'a str,
                    #[influxdb(tag)]
                    name: &'a str,
                    #[influxdb(tag)]
                    user: Option<&'a str>,
                    #[influxdb(field)]
                    value: $write,
                }

                impl<'a> From<[< $name Data >]<'a>> for Data<'a> {
                    fn from(value: [< $name Data >]<'a>) -> Self {
                        Self::$name(value)
                    }
                }
            )*

            enum Data<'a> {
                $(
                    $name([< $name Data >]<'a>),
                )*
            }

            impl<'a> Data<'a> {
                fn from_json(
                    client: &'a Client,
                    field: &'a ModelFieldNativeSpec,
                    value: &'a Value,
                    time: i64,
                ) -> Option<Vec<Self>> {
                    match &field.kind {
                        // BEGIN primitive types
                        ModelFieldKindNativeSpec::None {} => None,
                        // BEGIN primitive types
                        // BEGIN string formats
                        $(
                            $(
                                ModelFieldKindNativeSpec::$name { .. } => {
                                    let name: Vec<_> = field.name.split('/').skip(1).collect();
                                    let cast = |value| $cast(value)
                                        .map(|value| [< $name Data >] {
                                            time,
                                            namespace: &client.namespace,
                                            name: &field.name,
                                            user: None,
                                            value,
                                        })
                                        .map(Into::into);

                                    let mut output: Vec<_> = Default::default();
                                    get_json_children(&mut output, &name, value, cast);
                                    Some(output)
                                }
                            )?
                        )*
                        // BEGIN aggregation types
                        ModelFieldKindNativeSpec::Object { .. } => None,
                        ModelFieldKindNativeSpec::ObjectArray { .. } => None,
                    }
                }
            }

            impl<'a> WriteDataPoint for Data<'a> {
                /// Write this data point as line protocol. The implementor is responsible
                /// for properly escaping the data and ensuring that complete lines
                /// are generated.
                fn write_data_point_to<W>(&self, w: W) -> io::Result<()>
                where
                    W: io::Write,
                {
                    match self {
                        $(
                            Self::$name(value) => WriteDataPoint::write_data_point_to(value, w),
                        )*
                    }
                }
            }
        }

        impl FromMap for DynamicQuery {
            fn from_genericmap(mut map: GenericMap) -> Self {
                #[allow(non_snake_case)]
                mod parse {
                    use influxdb2_structmap::value::Value;

                    use super::{DateTime, FixedOffset, GenericMap};

                    // BEGIN primitive types

                    pub(super) fn None(_map: &mut GenericMap, _key: &'static str) -> () {}

                    pub(super) fn Boolean(map: &mut GenericMap, key: &'static str) -> bool {
                        map.remove(key)
                            .and_then(|value| match value {
                                Value::Bool(value) => Some(value),
                                _ => Option::None,
                            })
                            .unwrap_or_default()
                    }

                    pub(super) fn Integer(map: &mut GenericMap, key: &'static str) -> i64 {
                        map.remove(key)
                            .and_then(|value| match value {
                                Value::Long(value) => Some(value),
                                _ => Option::None,
                            })
                            .unwrap_or_default()
                    }

                    pub(super) fn Number(map: &mut GenericMap, key: &'static str) -> f64 {
                        map.remove(key)
                            .and_then(|value| match value {
                                Value::Double(value) => Some(*value),
                                _ => Option::None,
                            })
                            .unwrap_or_default()
                    }

                    pub(super) fn String(map: &mut GenericMap, key: &'static str) -> String {
                        map.remove(key)
                            .and_then(|value| match value {
                                Value::String(value) => Some(value),
                                _ => Option::None,
                            })
                            .unwrap_or_default()
                    }

                    pub(super) fn OneOfStrings(map: &mut GenericMap, key: &'static str) -> String {
                        String(map, key)
                    }

                    // BEGIN string formats

                    pub(super) fn DateTime(map: &mut GenericMap, key: &'static str) -> DateTime<FixedOffset> {
                        map.remove(key)
                            .and_then(|value| match value {
                                Value::TimeRFC(value) => Some(value),
                                _ => Option::None,
                            })
                            .unwrap_or_default()
                    }

                    pub(super) fn Ip(map: &mut GenericMap, key: &'static str) -> String {
                        String(map, key)
                    }

                    pub(super) fn Uuid(map: &mut GenericMap, key: &'static str) -> String {
                        String(map, key)
                    }
                }

                Self {
                    time: parse::DateTime(&mut map, "_time"),
                    namespace: parse::String(&mut map, "namespace"),
                    name: parse::String(&mut map, "name"),
                    user: parse::String(&mut map, "user"),
                    value: match parse::String(&mut map, "_measurement").as_str() {
                        $(
                            concat!( stringify!($name), "Data" ) => {
                                DynamicValue::$name(parse::$name(&mut map, "value"))
                            }
                        )*
                        _ => DynamicValue::None(()),
                    },
                }
            }
        }
    };
}

define_data!(
    // BEGIN primitive types
    None => write as bool,
    Boolean => query as bool => write as bool => cast as Value::as_bool,
    Integer => query as i64 => write as i64 => cast as Value::as_i64,
    Number => query as f64 => write as f64 => cast as Value::as_f64,
    String => query as String => write as &'a str => cast as Value::as_str,
    OneOfStrings => query as String => write as &'a str => cast as Value::as_str,
    // BEGIN string formats
    DateTime => query as DateTime<FixedOffset> => write as i64 => cast as Value::as_i64,
    Ip => query as String => write as &'a str => cast as Value::as_str,
    Uuid => query as String => write as &'a str => cast as Value::as_str,
);

fn get_json_children<'key, 'value, Output>(
    output: &mut Vec<Output>,
    name: &[&str],
    value: &'value Value,
    cast: impl Copy + Fn(&'value Value) -> Option<Output>,
) {
    match value {
        Value::Array(array) => array
            .iter()
            .rev()
            .for_each(|value| get_json_children(output, name, value, cast)),
        Value::Object(object) => {
            if let Some(&key) = name.first() {
                if let Some(value) = object.get(key) {
                    get_json_children(output, &name[1..], value, cast);
                }
            }
        }
        value => {
            if name.len() == 1 {
                if let Some(value) = cast(value) {
                    output.push(value);
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct DynamicQuery {
    pub time: DateTime<FixedOffset>,
    pub namespace: String,
    pub name: String,
    pub user: String,
    pub value: DynamicValue,
}

impl DynamicQuery {
    pub const fn builder<'a>() -> QueryBuilder<'a, Self> {
        QueryBuilder {
            _output: PhantomData,
            options: QueryOptions {
                filter_measurement: None,
                filter_namespace: None,
                filter_name: None,
                filter_user: None,
                group: None,
                range: QueryRange {
                    start: None,
                    end: None,
                },
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum DynamicValue {
    // BEGIN primitive types
    None(()),
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
    OneOfStrings(String),
    // BEGIN string formats
    DateTime(DateTime<FixedOffset>),
    Ip(String),
    Uuid(String),
}

impl Default for DynamicValue {
    fn default() -> Self {
        Self::None(())
    }
}

impl Default for &DynamicValue {
    fn default() -> Self {
        &DynamicValue::None(())
    }
}

impl fmt::Display for DynamicValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // BEGIN primitive types
            Self::None(()) => write!(f, "None"),
            Self::Boolean(value) => value.fmt(f),
            Self::Integer(value) => value.fmt(f),
            Self::Number(value) => value.fmt(f),
            Self::String(value) => value.fmt(f),
            Self::OneOfStrings(value) => value.fmt(f),
            // BEGIN string formats
            Self::DateTime(value) => value.fmt(f),
            Self::Ip(value) => value.fmt(f),
            Self::Uuid(value) => value.fmt(f),
        }
    }
}
