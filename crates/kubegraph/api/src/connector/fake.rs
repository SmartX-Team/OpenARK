use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConnectorFakeSpec {
    #[serde(default)]
    pub edges: Option<NetworkConnectorFakeData>,
    #[serde(default)]
    pub nodes: Option<NetworkConnectorFakeData>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConnectorFakeData {
    #[serde(default = "NetworkConnectorFakeData::default_count")]
    #[validate(range(min = 1))]
    pub count: usize,
    #[serde(default)]
    pub frame: NetworkConnectorFakeDataFrame,
}

impl NetworkConnectorFakeData {
    const fn default_count() -> usize {
        1
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent, rename_all = "camelCase")]
pub struct NetworkConnectorFakeDataFrame {
    pub map: BTreeMap<String, NetworkConnectorFakeDataModel>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NetworkConnectorFakeDataModel {
    Constant(#[serde(default)] self::model::ConstantModel),
    Name(#[serde(default)] self::model::NameModel),
    Normal(#[serde(default)] self::model::NormalModel),
}

impl NetworkConnectorFakeDataModel {
    const fn default_mean() -> f64 {
        0.0
    }

    const fn default_prefix() -> Option<String> {
        None
    }

    const fn default_seed() -> Option<u64> {
        None
    }

    const fn default_std() -> f64 {
        1.0
    }

    const fn default_value() -> f64 {
        0.0
    }
}

mod impl_json_schema_for_fake_data_model {
    use std::borrow::Cow;

    use schemars::{gen::SchemaGenerator, schema::Schema, JsonSchema};

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    enum NetworkConnectorFakeDataModelType {
        Constant,
        Name,
        Normal,
    }

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(rename_all = "camelCase")]
    struct NetworkConnectorFakeDataModel {
        #[serde(default = "super::NetworkConnectorFakeDataModel::default_mean")]
        mean: f64,
        #[serde(default = "super::NetworkConnectorFakeDataModel::default_prefix")]
        #[validate(length(min = 1, max = 32))]
        prefix: Option<String>,
        r#type: NetworkConnectorFakeDataModelType,
        #[serde(default = "super::NetworkConnectorFakeDataModel::default_seed")]
        seed: Option<u64>,
        #[serde(default = "super::NetworkConnectorFakeDataModel::default_std")]
        #[validate(range(min = 0.0))]
        std: f64,
        #[serde(default = "super::NetworkConnectorFakeDataModel::default_value")]
        value: f64,
        #[serde(default)]
        value_type: super::NetworkConnectorFakeDataValueType,
    }

    impl JsonSchema for super::NetworkConnectorFakeDataModel {
        #[inline]
        fn is_referenceable() -> bool {
            <NetworkConnectorFakeDataModel as JsonSchema>::is_referenceable()
        }

        #[inline]
        fn schema_name() -> String {
            <NetworkConnectorFakeDataModel as JsonSchema>::schema_name()
        }

        #[inline]
        fn json_schema(gen: &mut SchemaGenerator) -> Schema {
            <NetworkConnectorFakeDataModel as JsonSchema>::json_schema(gen)
        }

        #[inline]
        fn schema_id() -> Cow<'static, str> {
            <NetworkConnectorFakeDataModel as JsonSchema>::schema_id()
        }
    }
}

pub mod model {
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct ConstantModel {
        #[serde(default = "super::NetworkConnectorFakeDataModel::default_value")]
        pub value: f64,
        #[serde(default)]
        pub value_type: super::NetworkConnectorFakeDataValueType,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct NameModel {
        #[serde(default = "super::NetworkConnectorFakeDataModel::default_prefix")]
        #[validate(length(min = 1, max = 32))]
        pub prefix: Option<String>,
    }

    #[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct NormalModel {
        #[serde(default = "super::NetworkConnectorFakeDataModel::default_mean")]
        pub mean: f64,
        #[serde(default = "super::NetworkConnectorFakeDataModel::default_seed")]
        pub seed: Option<u64>,
        #[serde(default = "super::NetworkConnectorFakeDataModel::default_std")]
        #[validate(range(min = 0.0))]
        pub std: f64,
        #[serde(default)]
        pub value_type: super::NetworkConnectorFakeDataValueType,
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum NetworkConnectorFakeDataValueType {
    #[default]
    F64,
    I64,
}

#[cfg(feature = "df-polars")]
impl From<NetworkConnectorFakeDataValueType> for ::pl::datatypes::DataType {
    fn from(value: NetworkConnectorFakeDataValueType) -> Self {
        match value {
            NetworkConnectorFakeDataValueType::F64 => Self::Float64,
            NetworkConnectorFakeDataValueType::I64 => Self::Int64,
        }
    }
}
