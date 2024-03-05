use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQuery {
    pub query: String,
    #[serde(
        deserialize_with = "self::prefix::sink::deserialize",
        serialize_with = "self::prefix::sink::serialize"
    )]
    pub sink: NetworkQueryNodeType,
    #[serde(
        deserialize_with = "self::prefix::src::deserialize",
        serialize_with = "self::prefix::src::serialize"
    )]
    pub src: NetworkQueryNodeType,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQueryNodeType {
    #[serde(
        deserialize_with = "self::prefix::kind::deserialize",
        serialize_with = "self::prefix::kind::serialize"
    )]
    pub kind: NetworkQueryNodeValue,
    #[serde(
        deserialize_with = "self::prefix::name::deserialize",
        serialize_with = "self::prefix::name::serialize"
    )]
    pub name: NetworkQueryNodeValue,
    #[serde(
        deserialize_with = "self::prefix::namespace::deserialize",
        serialize_with = "self::prefix::namespace::serialize"
    )]
    pub namespace: NetworkQueryNodeValue,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase", tag = "type", content = "value")]
pub enum NetworkQueryNodeValue {
    Key(String),
    Static(#[serde(default, skip_serializing_if = "Option::is_none")] Option<String>),
}

mod prefix {
    ::serde_with::with_prefix!(pub(super) sink "sink_");
    ::serde_with::with_prefix!(pub(super) src "src_");

    ::serde_with::with_prefix!(pub(super) kind "kind_");
    ::serde_with::with_prefix!(pub(super) name "name_");
    ::serde_with::with_prefix!(pub(super) namespace "namespace_");
}
