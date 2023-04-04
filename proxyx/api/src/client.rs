use std::collections::BTreeMap;

use dash_api::{
    model::{ModelFieldKindNativeSpec, ModelFieldNativeSpec},
    serde_json::Value,
};
use ipis::{
    core::{anyhow::Result, chrono::Duration, value::text::LanguageTag},
    itertools::Itertools,
};
use netai_api::nlp::QuestionWordInputRef;
use serde::{Deserialize, Serialize};
use storage::{DynamicQuery, DynamicValue};

pub struct Client {
    netai_nlp_question_answering: ::netai_client::Client,
    storage: crate::storage::Client,
    fields: FieldMap,
}

impl Client {
    pub async fn try_default() -> Result<Self> {
        let mut client = Self {
            netai_nlp_question_answering: ::netai_client::Client::with_env(
                "NETAI_HOST_NLP_QUESTION_ANSWERING",
            )?,
            storage: crate::storage::Client::try_default()?,
            fields: Self::load_fields().await?,
        };

        client.reload_cache().await.map(|()| client)
    }

    async fn load_fields() -> Result<FieldMap> {
        Ok(Default::default())
    }

    pub async fn question(&self, name: &str, context: &str) -> Result<Option<FieldLabel>> {
        let field = match self.fields.get(name) {
            Some(field) => field,
            None => return Ok(None),
        };
        let inputs = &[QuestionWordInputRef {
            context,
            question: {
                let name = field
                    .metadata
                    .name
                    .split('/')
                    .rev()
                    .filter(|key| !key.is_empty())
                    .join(" of ");
                // let type_ = match field.metadata.kind.to_type() {
                //     // BEGIN primitive types
                //     ModelFieldKindNativeType::None => return Ok(None),
                //     ModelFieldKindNativeType::Boolean => "boolean",
                //     ModelFieldKindNativeType::Integer => "integer",
                //     ModelFieldKindNativeType::Number => "number",
                //     ModelFieldKindNativeType::String => "word",
                //     ModelFieldKindNativeType::OneOfStrings => "word",
                //     // BEGIN string formats
                //     ModelFieldKindNativeType::DateTime => "date time",
                //     ModelFieldKindNativeType::Ip => "IP",
                //     ModelFieldKindNativeType::Uuid => "UUID",
                //     // BEGIN aggregation types
                //     ModelFieldKindNativeType::Object => return Ok(None),
                //     ModelFieldKindNativeType::ObjectArray => return Ok(None),
                // };

                &[&format!("What is {name}?")]
            },
        }];

        let mut outputs = self
            .netai_nlp_question_answering
            .call_question_answering(inputs)
            .await?;

        Ok(outputs
            .pop()
            .and_then(|mut question| question.pop())
            .filter(|answer| !answer.is_empty())
            .map(|label| FieldLabel {
                label,
                language: LanguageTag::new_en_us(),
            }))
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldLabel {
    pub label: String,
    pub language: LanguageTag,
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
