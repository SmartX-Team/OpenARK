use ipis::core::{
    anyhow::{bail, Result},
    chrono::{DateTime, Utc},
};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, CustomResource)]
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
    }"#
)]
#[serde(rename_all = "camelCase")]
pub enum ModelSpec {
    Fields(ModelFieldsSpec),
    CustomResourceDefinitionRef(ModelCustomResourceDefinitionRefSpec),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub state: Option<ModelState>,
    pub fields: Option<ModelFieldsSpec<ModelFieldKindNativeSpec>>,
    pub last_updated: DateTime<Utc>,
}

pub type ModelFieldsSpec<Kind = ModelFieldKindSpec> = Vec<ModelFieldSpec<Kind>>;
pub type ModelFieldsNativeSpec = ModelFieldsSpec<ModelFieldKindNativeSpec>;

pub type ModelFieldNativeSpec = ModelFieldSpec<ModelFieldKindNativeSpec>;
pub type ModelFieldExtendedSpec = ModelFieldSpec<ModelFieldKindExtendedSpec>;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelFieldSpec<Kind = ModelFieldKindSpec> {
    pub name: String,
    #[serde(flatten)]
    pub kind: Kind,
    #[serde(default)]
    pub optional: bool,
}

impl ModelFieldSpec {
    pub fn try_into_native(self) -> Result<ModelFieldNativeSpec> {
        match self.kind {
            ModelFieldKindSpec::Native(kind) => Ok(ModelFieldSpec {
                name: self.name,
                kind,
                optional: self.optional,
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
                optional: self.optional,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
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
    #[allow(dead_code)]
    #[derive(super::JsonSchema)]
    enum ModelFieldKindSpec {
        Native(super::ModelFieldKindNativeSpec),
        Extended(super::ModelFieldKindExtendedSpec),
    }

    impl super::JsonSchema for super::ModelFieldKindSpec {
        fn is_referenceable() -> bool {
            <ModelFieldKindSpec as super::JsonSchema>::is_referenceable()
        }

        fn schema_name() -> String {
            <ModelFieldKindSpec as super::JsonSchema>::schema_name()
        }

        fn json_schema(gen: &mut ::schemars::gen::SchemaGenerator) -> ::schemars::schema::Schema {
            fn merge_schema_object(
                subject: &mut ::schemars::schema::SchemaObject,
                object: ::schemars::schema::Schema,
            ) {
                let object = match object {
                    ::schemars::schema::Schema::Object(object) => object,
                    _ => unreachable!("should be typed"),
                };

                match &mut subject.subschemas {
                    Some(subject) => {
                        let subject = subject.one_of.as_mut().unwrap();
                        let mut object = object.subschemas.unwrap().one_of.unwrap();
                        subject.append(&mut object);
                    }
                    subject => *subject = object.subschemas,
                }
            }

            let mut schema = ::schemars::schema::SchemaObject::default();
            merge_schema_object(
                &mut schema,
                super::ModelFieldKindNativeSpec::json_schema(gen),
            );
            merge_schema_object(
                &mut schema,
                super::ModelFieldKindExtendedSpec::json_schema(gen),
            );
            ::schemars::schema::Schema::Object(schema)
        }

        fn _schemars_private_is_option() -> bool {
            <ModelFieldKindSpec as super::JsonSchema>::_schemars_private_is_option()
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
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
            Self::Object { .. } => ModelFieldKindNativeType::Object,
            Self::ObjectArray { .. } => ModelFieldKindNativeType::ObjectArray,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
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
    Object,
    ObjectArray,
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelCustomResourceDefinitionRefSpec {
    pub name: String,
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
