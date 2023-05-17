use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "Model",
    struct = "ModelCrd",
    status = "ModelStatus",
    shortname = "m",
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description":"state of the model",
        "jsonPath":".status.state"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description":"created time",
        "jsonPath":".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "updated-at",
        "type": "date",
        "description":"updated time",
        "jsonPath":".status.lastUpdated"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "date",
        "description":"model version",
        "jsonPath":".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub enum ModelSpec {
    Fields(ModelFieldsSpec),
    CustomResourceDefinitionRef(ModelCustomResourceDefinitionRefSpec),
}

impl ModelCrd {
    pub fn get_fields_unchecked(&self) -> &ModelFieldsNativeSpec {
        self.status
            .as_ref()
            .and_then(|status| status.fields.as_ref())
            .expect("fields should not be empty")
    }

    pub fn into_fields_unchecked(self) -> ModelFieldsNativeSpec {
        self.status
            .and_then(|status| status.fields)
            .expect("fields should not be empty")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    #[serde(default)]
    pub state: ModelState,
    pub fields: Option<ModelFieldsSpec<ModelFieldKindNativeSpec>>,
    pub last_updated: DateTime<Utc>,
}

pub type ModelFieldsSpec<Kind = ModelFieldKindSpec> = Vec<ModelFieldSpec<Kind>>;
pub type ModelFieldsNativeSpec = ModelFieldsSpec<ModelFieldKindNativeSpec>;

pub type ModelFieldNativeSpec = ModelFieldSpec<ModelFieldKindNativeSpec>;
pub type ModelFieldExtendedSpec = ModelFieldSpec<ModelFieldKindExtendedSpec>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelFieldSpec<Kind = ModelFieldKindSpec> {
    pub name: String,
    #[serde(flatten)]
    pub kind: Kind,
    #[serde(flatten)]
    pub attribute: ModelFieldAttributeSpec,
}

impl ModelFieldSpec {
    pub fn try_into_native(self) -> Result<ModelFieldNativeSpec> {
        match self.kind {
            ModelFieldKindSpec::Native(kind) => Ok(ModelFieldSpec {
                name: self.name,
                kind,
                attribute: self.attribute,
            }),
            kind => {
                let name = &self.name;
                let type_ = kind.to_type();
                bail!(
                    "cannot infer field type {name:?}: expected Native types, but given {type_:?}"
                )
            }
        }
    }

    pub fn try_into_extended(self) -> Result<ModelFieldExtendedSpec> {
        match self.kind {
            ModelFieldKindSpec::Extended(kind) => Ok(ModelFieldSpec {
                name: self.name,
                kind,
                attribute: self.attribute,
            }),
            kind => {
                let name = &self.name;
                let type_ = kind.to_type();
                bail!(
                    "cannot infer field type {name:?}: expected Extended types, but given {type_:?}"
                )
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelFieldAttributeSpec {
    #[serde(default)]
    pub optional: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModelFieldKindSpec {
    Native(ModelFieldKindNativeSpec),
    Extended(ModelFieldKindExtendedSpec),
}

impl Default for ModelFieldKindSpec {
    fn default() -> Self {
        Self::Native(Default::default())
    }
}

// NOTE: JsonSchema has no support for merging two enums
mod _impl_jsonschema_for_model_field_kind_spec {
    use super::*;

    mod serialize {
        #[allow(dead_code)]
        #[derive(super::Serialize)]
        #[serde(rename_all = "camelCase")]
        enum ModelFieldKindSpec<'a> {
            // BEGIN primitive types
            None {},
            Boolean {
                #[serde(default)]
                default: &'a Option<bool>,
            },
            Integer {
                #[serde(default)]
                default: &'a Option<i64>,
                #[serde(default)]
                minimum: &'a Option<i64>,
                #[serde(default)]
                maximum: &'a Option<i64>,
            },
            Number {
                #[serde(default)]
                default: &'a Option<f64>,
                #[serde(default)]
                minimum: &'a Option<f64>,
                #[serde(default)]
                maximum: &'a Option<f64>,
            },
            String {
                #[serde(default)]
                default: &'a Option<String>,
                #[serde(default, flatten)]
                kind: &'a super::ModelFieldKindStringSpec,
            },
            OneOfStrings {
                #[serde(default)]
                default: &'a Option<String>,
                choices: &'a Vec<String>,
            },
            // BEGIN string formats
            DateTime {
                #[serde(default)]
                default: &'a Option<super::ModelFieldDateTimeDefaultType>,
            },
            Ip {},
            Uuid {},
            // BEGIN aggregation types
            StringArray {},
            Object {
                #[serde(default)]
                children: &'a Vec<String>,
                #[serde(default)]
                dynamic: &'a bool,
            },
            ObjectArray {
                #[serde(default)]
                children: &'a Vec<String>,
            },
            // BEGIN reference types
            Model {
                name: &'a String,
            },
        }

        impl super::Serialize for super::ModelFieldKindSpec {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                let spec = match self {
                    Self::Native(spec) => match spec {
                        // BEGIN primitive types
                        super::ModelFieldKindNativeSpec::None {} => ModelFieldKindSpec::None {},
                        super::ModelFieldKindNativeSpec::Boolean { default } => {
                            ModelFieldKindSpec::Boolean { default }
                        }
                        super::ModelFieldKindNativeSpec::Integer {
                            default,
                            minimum,
                            maximum,
                        } => ModelFieldKindSpec::Integer {
                            default,
                            minimum,
                            maximum,
                        },
                        super::ModelFieldKindNativeSpec::Number {
                            default,
                            minimum,
                            maximum,
                        } => ModelFieldKindSpec::Number {
                            default,
                            minimum,
                            maximum,
                        },
                        super::ModelFieldKindNativeSpec::String { default, kind } => {
                            ModelFieldKindSpec::String { default, kind }
                        }
                        super::ModelFieldKindNativeSpec::OneOfStrings { default, choices } => {
                            ModelFieldKindSpec::OneOfStrings { default, choices }
                        }
                        // BEGIN string formats
                        super::ModelFieldKindNativeSpec::DateTime { default } => {
                            ModelFieldKindSpec::DateTime { default }
                        }
                        super::ModelFieldKindNativeSpec::Ip {} => ModelFieldKindSpec::Ip {},
                        super::ModelFieldKindNativeSpec::Uuid {} => ModelFieldKindSpec::Uuid {},
                        // BEGIN aggregation types
                        super::ModelFieldKindNativeSpec::StringArray {} => {
                            ModelFieldKindSpec::StringArray {}
                        }
                        super::ModelFieldKindNativeSpec::Object { children, dynamic } => {
                            ModelFieldKindSpec::Object { children, dynamic }
                        }
                        super::ModelFieldKindNativeSpec::ObjectArray { children } => {
                            ModelFieldKindSpec::ObjectArray { children }
                        }
                    },
                    Self::Extended(spec) => match spec {
                        // BEGIN reference types
                        super::ModelFieldKindExtendedSpec::Model { name } => {
                            ModelFieldKindSpec::Model { name }
                        }
                    },
                };

                spec.serialize(serializer)
            }
        }
    }

    mod deserialize {
        #[allow(dead_code)]
        #[derive(super::Deserialize, super::JsonSchema)]
        #[serde(rename_all = "camelCase")]
        enum ModelFieldKindSpec {
            // BEGIN primitive types
            None {},
            Boolean {
                #[serde(default)]
                default: Option<bool>,
            },
            Integer {
                #[serde(default)]
                default: Option<i64>,
                #[serde(default)]
                minimum: Option<i64>,
                #[serde(default)]
                maximum: Option<i64>,
            },
            Number {
                #[serde(default)]
                default: Option<f64>,
                #[serde(default)]
                minimum: Option<f64>,
                #[serde(default)]
                maximum: Option<f64>,
            },
            String {
                #[serde(default)]
                default: Option<String>,
                #[serde(default, flatten)]
                kind: super::ModelFieldKindStringSpec,
            },
            OneOfStrings {
                #[serde(default)]
                default: Option<String>,
                choices: Vec<String>,
            },
            // BEGIN string formats
            DateTime {
                #[serde(default)]
                default: Option<super::ModelFieldDateTimeDefaultType>,
            },
            Ip {},
            Uuid {},
            // BEGIN aggregation types
            Object {
                #[serde(default)]
                children: Vec<String>,
                #[serde(default)]
                dynamic: bool,
            },
            ObjectArray {
                #[serde(default)]
                children: Vec<String>,
            },
            // BEGIN reference types
            Model {
                name: String,
            },
        }

        impl<'de> super::Deserialize<'de> for super::ModelFieldKindSpec {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                <ModelFieldKindSpec as super::Deserialize<'de>>::deserialize(deserializer).map(
                    |spec| {
                        match spec {
                            // BEGIN primitive types
                            ModelFieldKindSpec::None {} => {
                                Self::Native(super::ModelFieldKindNativeSpec::None {})
                            }
                            ModelFieldKindSpec::Boolean { default } => {
                                Self::Native(super::ModelFieldKindNativeSpec::Boolean { default })
                            }
                            ModelFieldKindSpec::Integer {
                                default,
                                minimum,
                                maximum,
                            } => Self::Native(super::ModelFieldKindNativeSpec::Integer {
                                default,
                                minimum,
                                maximum,
                            }),
                            ModelFieldKindSpec::Number {
                                default,
                                minimum,
                                maximum,
                            } => Self::Native(super::ModelFieldKindNativeSpec::Number {
                                default,
                                minimum,
                                maximum,
                            }),
                            ModelFieldKindSpec::String { default, kind } => {
                                Self::Native(super::ModelFieldKindNativeSpec::String {
                                    default,
                                    kind,
                                })
                            }
                            ModelFieldKindSpec::OneOfStrings { default, choices } => {
                                Self::Native(super::ModelFieldKindNativeSpec::OneOfStrings {
                                    default,
                                    choices,
                                })
                            }
                            // BEGIN string formats
                            ModelFieldKindSpec::DateTime { default } => {
                                Self::Native(super::ModelFieldKindNativeSpec::DateTime { default })
                            }
                            ModelFieldKindSpec::Ip {} => {
                                Self::Native(super::ModelFieldKindNativeSpec::Ip {})
                            }
                            ModelFieldKindSpec::Uuid {} => {
                                Self::Native(super::ModelFieldKindNativeSpec::Uuid {})
                            }
                            // BEGIN aggregation types
                            ModelFieldKindSpec::Object { children, dynamic } => {
                                Self::Native(super::ModelFieldKindNativeSpec::Object {
                                    children,
                                    dynamic,
                                })
                            }
                            ModelFieldKindSpec::ObjectArray { children } => {
                                Self::Native(super::ModelFieldKindNativeSpec::ObjectArray {
                                    children,
                                })
                            }
                            // BEGIN reference types
                            ModelFieldKindSpec::Model { name } => {
                                Self::Extended(super::ModelFieldKindExtendedSpec::Model { name })
                            }
                        }
                    },
                )
            }
        }

        impl super::JsonSchema for super::ModelFieldKindSpec {
            fn is_referenceable() -> bool {
                <ModelFieldKindSpec as super::JsonSchema>::is_referenceable()
            }

            fn schema_name() -> String {
                <ModelFieldKindSpec as super::JsonSchema>::schema_name()
            }

            fn json_schema(
                gen: &mut ::schemars::gen::SchemaGenerator,
            ) -> ::schemars::schema::Schema {
                <ModelFieldKindSpec as super::JsonSchema>::json_schema(gen)
            }

            fn _schemars_private_non_optional_json_schema(
                gen: &mut ::schemars::gen::SchemaGenerator,
            ) -> ::schemars::schema::Schema {
                <ModelFieldKindSpec as super::JsonSchema>::_schemars_private_non_optional_json_schema(gen)
            }

            fn _schemars_private_is_option() -> bool {
                <ModelFieldKindSpec as super::JsonSchema>::_schemars_private_is_option()
            }
        }
    }
}

impl ModelFieldKindSpec {
    pub const fn to_type(&self) -> ModelFieldKindType {
        match self {
            Self::Native(spec) => ModelFieldKindType::Native(spec.to_type()),
            Self::Extended(spec) => ModelFieldKindType::Extended(spec.to_type()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelFieldKindNativeSpec {
    // BEGIN primitive types
    None {},
    Boolean {
        #[serde(default)]
        default: Option<bool>,
    },
    Integer {
        #[serde(default)]
        default: Option<i64>,
        #[serde(default)]
        minimum: Option<i64>,
        #[serde(default)]
        maximum: Option<i64>,
    },
    Number {
        #[serde(default)]
        default: Option<f64>,
        #[serde(default)]
        minimum: Option<f64>,
        #[serde(default)]
        maximum: Option<f64>,
    },
    String {
        #[serde(default)]
        default: Option<String>,
        #[serde(default, flatten)]
        kind: ModelFieldKindStringSpec,
    },
    OneOfStrings {
        #[serde(default)]
        default: Option<String>,
        choices: Vec<String>,
    },
    // BEGIN string formats
    DateTime {
        #[serde(default)]
        default: Option<ModelFieldDateTimeDefaultType>,
    },
    Ip {},
    Uuid {},
    // BEGIN aggregation types
    StringArray {},
    Object {
        #[serde(default)]
        children: Vec<String>,
        #[serde(default)]
        dynamic: bool,
    },
    ObjectArray {
        #[serde(default)]
        children: Vec<String>,
    },
}

impl Default for ModelFieldKindNativeSpec {
    fn default() -> Self {
        Self::None {}
    }
}

impl ModelFieldKindNativeSpec {
    pub fn get_children(&self) -> Option<&Vec<String>> {
        match self {
            // BEGIN primitive types
            Self::None {} => None,
            Self::Boolean { .. } => None,
            Self::Integer { .. } => None,
            Self::Number { .. } => None,
            Self::String { .. } => None,
            Self::OneOfStrings { .. } => None,
            // BEGIN string formats
            Self::DateTime { .. } => None,
            Self::Ip { .. } => None,
            Self::Uuid { .. } => None,
            // BEGIN aggregation types
            Self::StringArray { .. } => None,
            Self::Object { children, .. } | Self::ObjectArray { children, .. } => Some(children),
        }
    }

    pub fn get_children_mut(&mut self) -> Option<&mut Vec<String>> {
        match self {
            // BEGIN primitive types
            Self::None {} => None,
            Self::Boolean { .. } => None,
            Self::Integer { .. } => None,
            Self::Number { .. } => None,
            Self::String { .. } => None,
            Self::OneOfStrings { .. } => None,
            // BEGIN string formats
            Self::DateTime { .. } => None,
            Self::Ip { .. } => None,
            Self::Uuid { .. } => None,
            // BEGIN aggregation types
            Self::StringArray { .. } => None,
            Self::Object { children, .. } | Self::ObjectArray { children, .. } => Some(children),
        }
    }

    pub const fn to_type(&self) -> ModelFieldKindNativeType {
        match self {
            // BEGIN primitive types
            Self::None {} => ModelFieldKindNativeType::None,
            Self::Boolean { .. } => ModelFieldKindNativeType::Boolean,
            Self::Integer { .. } => ModelFieldKindNativeType::Integer,
            Self::Number { .. } => ModelFieldKindNativeType::Number,
            Self::String { .. } => ModelFieldKindNativeType::String,
            Self::OneOfStrings { .. } => ModelFieldKindNativeType::OneOfStrings,
            // BEGIN string formats
            Self::DateTime { .. } => ModelFieldKindNativeType::DateTime,
            Self::Ip { .. } => ModelFieldKindNativeType::Ip,
            Self::Uuid { .. } => ModelFieldKindNativeType::Uuid,
            // BEGIN aggregation types
            Self::StringArray { .. } => ModelFieldKindNativeType::StringArray,
            Self::Object { .. } => ModelFieldKindNativeType::Object,
            Self::ObjectArray { .. } => ModelFieldKindNativeType::ObjectArray,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelFieldKindStringSpec {
    Dynamic {},
    Static {
        length: u32,
    },
    Range {
        #[serde(default)]
        minimum: Option<u32>,
        maximum: u32,
    },
}

impl Default for ModelFieldKindStringSpec {
    fn default() -> Self {
        Self::Dynamic {}
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelFieldKindExtendedSpec {
    // BEGIN reference types
    Model { name: String },
}

impl ModelFieldKindExtendedSpec {
    pub const fn to_type(&self) -> ModelFieldKindExtendedType {
        match self {
            // BEGIN reference types
            Self::Model { .. } => ModelFieldKindExtendedType::Model,
        }
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(untagged)]
pub enum ModelFieldKindType {
    Native(ModelFieldKindNativeType),
    Extended(ModelFieldKindExtendedType),
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum ModelFieldKindNativeType {
    // BEGIN primitive types
    None,
    Boolean,
    Integer,
    Number,
    String,
    OneOfStrings,
    // BEGIN string formats
    DateTime,
    Ip,
    Uuid,
    // BEGIN aggregation types
    StringArray,
    Object,
    ObjectArray,
}

impl ModelFieldKindNativeType {
    pub fn to_natural(&self) -> &'static str {
        match self {
            // BEGIN primitive types
            Self::None => "None",
            Self::Boolean => "Boolean",
            Self::Integer => "Integer",
            Self::Number => "Number",
            Self::String | Self::OneOfStrings => "String",
            // BEGIN string formats
            Self::DateTime => "DateTime",
            Self::Ip => "Ip",
            Self::Uuid => "Uuid",
            // BEGIN aggregation types
            Self::StringArray => "String[]",
            Self::Object => "Object",
            Self::ObjectArray => "Object[]",
        }
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum ModelFieldKindExtendedType {
    // BEGIN reference types
    Model,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum ModelFieldDateTimeDefaultType {
    Now,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelCustomResourceDefinitionRefSpec {
    pub name: String,
}

impl ModelCustomResourceDefinitionRefSpec {
    pub fn plural(&self) -> &str {
        self.name.split('.').next().unwrap()
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum ModelState {
    Pending,
    Ready,
}

impl Default for ModelState {
    fn default() -> Self {
        Self::Pending
    }
}
