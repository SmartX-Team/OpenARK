use std::collections::BTreeMap;

use dash_api::{
    model::{ModelFieldKindNativeSpec, ModelFieldNativeSpec},
    serde_json::Value,
};
use ipis::core::{anyhow::Result, chrono::Duration};
use storage::{DynamicQuery, DynamicValue};

pub struct Client {
    storage: crate::storage::Client,
    fields: FieldMap,
}

impl Client {
    pub async fn try_default() -> Result<Self> {
        let mut client = Self {
            storage: crate::storage::Client::try_default()?,
            fields: Self::load_fields().await?,
        };

        client.reload_cache().await.map(|()| client)
    }

    async fn load_fields() -> Result<FieldMap> {
        Ok(Default::default())
    }

    pub fn update_fields(&mut self, fields: Vec<ModelFieldNativeSpec>) {
        fields
            .into_iter()
            .for_each(|metadata| match self.fields.get_mut(&metadata.name) {
                Some(field) => {
                    field.metadata = metadata;
                }
                None => {
                    self.fields.insert(
                        metadata.name.clone(),
                        Field {
                            metadata,
                            value: Default::default(),
                        },
                    );
                }
            })
    }

    pub async fn add_json(&self, value: &Value) -> Result<()> {
        let fields = self.fields.values().map(|field| &field.metadata);
        self.storage.write_json(fields, value).await
    }

    pub fn get_raw(&self, name: impl AsRef<str>) -> &DynamicValue {
        self.fields
            .get(name.as_ref())
            .map(|field| &field.value)
            .unwrap_or_default()
    }

    pub async fn reload_cache(&mut self) -> Result<()> {
        DynamicQuery::builder()
            .start(Duration::days(365))
            .group(&["namespace", "name"])
            .last(&self.storage)
            .await
            .map(|query| {
                query
                    .into_iter()
                    .for_each(|query| match self.fields.get_mut(&query.name) {
                        Some(field) => {
                            field.value = query.value;
                        }
                        None => {
                            self.fields
                                .insert(query.name.clone(), Field::parse_value(query));
                        }
                    })
            })
    }
}

type FieldMap = BTreeMap<String, Field>;

struct Field {
    metadata: ModelFieldNativeSpec,
    value: DynamicValue,
}

impl Field {
    fn parse_value(query: DynamicQuery) -> Self {
        Self {
            metadata: ModelFieldNativeSpec {
                name: query.name,
                kind: match &query.value {
                    // BEGIN primitive types
                    DynamicValue::None(_) => ModelFieldKindNativeSpec::None {},
                    DynamicValue::Boolean(_) => ModelFieldKindNativeSpec::Boolean {
                        default: Default::default(),
                    },
                    DynamicValue::Integer(_) => ModelFieldKindNativeSpec::Integer {
                        default: Default::default(),
                        minimum: Default::default(),
                        maximum: Default::default(),
                    },
                    DynamicValue::Number(_) => ModelFieldKindNativeSpec::Number {
                        default: Default::default(),
                        minimum: Default::default(),
                        maximum: Default::default(),
                    },
                    DynamicValue::String(_) => ModelFieldKindNativeSpec::String {
                        default: Default::default(),
                        kind: Default::default(),
                    },
                    DynamicValue::OneOfStrings(_) => ModelFieldKindNativeSpec::OneOfStrings {
                        default: Default::default(),
                        choices: Default::default(),
                    },
                    // BEGIN string formats
                    DynamicValue::DateTime(_) => ModelFieldKindNativeSpec::DateTime {
                        default: Default::default(),
                    },
                    DynamicValue::Ip(_) => ModelFieldKindNativeSpec::Ip {},
                    DynamicValue::Uuid(_) => ModelFieldKindNativeSpec::Uuid {},
                },
                attribute: Default::default(),
            },
            value: query.value,
        }
    }
}
