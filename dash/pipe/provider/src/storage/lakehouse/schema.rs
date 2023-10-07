use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Error, Result};
use dash_api::model::{
    ModelFieldAttributeSpec, ModelFieldKindNativeSpec, ModelFieldKindObjectSpec,
    ModelFieldNativeSpec,
};
use datafusion::arrow::datatypes::DataType;
use deltalake::{SchemaDataType, SchemaField, SchemaTypeArray, SchemaTypeStruct};
use map_macro::hash_map;
use schemars::schema::{
    ArrayValidation, InstanceType, ObjectValidation, RootSchema, Schema, SchemaObject, SingleOrVec,
};
use serde_json::{json, Value};

pub trait FieldColumns {
    fn to_data_types(&self) -> Result<Vec<SchemaField>>;
}

impl FieldColumns for RootSchema {
    fn to_data_types(&self) -> Result<Vec<SchemaField>> {
        fn find_instance_type_none(instance_types: &[InstanceType]) -> Option<usize> {
            instance_types
                .iter()
                .enumerate()
                .find(|(_, instance_type)| matches!(instance_type, InstanceType::Null))
                .map(|(index, _)| index)
        }

        trait JsonFieldColumn {
            fn to_data_type(
                &self,
                api_version: &Value,
                definitions: &::schemars::Map<String, Schema>,
                name: &str,
                nullable: bool,
            ) -> Result<Option<SchemaField>>;
        }

        impl JsonFieldColumn for ::schemars::schema::Schema {
            fn to_data_type(
                &self,
                api_version: &Value,
                definitions: &schemars::Map<String, Schema>,
                name: &str,
                nullable: bool,
            ) -> Result<Option<SchemaField>> {
                struct Context<'a> {
                    api_version: &'a Value,
                    definitions: &'a schemars::Map<String, Schema>,
                    name: &'a str,
                }

                fn parse_instance_type(
                    Context {
                        api_version,
                        definitions,
                        name,
                    }: Context,
                    value: &SchemaObject,
                    instance_type: &InstanceType,
                    metadata: impl FnOnce(Value) -> FieldMetadata,
                    nullable: bool,
                ) -> Result<Option<SchemaField>> {
                    Ok(match instance_type {
                        InstanceType::Null => None,
                        InstanceType::Boolean => Some(SchemaField::new(
                            name.into(),
                            self::types::boolean(),
                            nullable,
                            metadata("Boolean".into()),
                        )),
                        InstanceType::Integer => Some(SchemaField::new(
                            name.into(),
                            self::types::integer(),
                            nullable,
                            metadata("Integer".into()),
                        )),
                        InstanceType::Number => Some(SchemaField::new(
                            name.into(),
                            self::types::number(),
                            nullable,
                            metadata("Number".into()),
                        )),
                        InstanceType::String => Some(SchemaField::new(
                            name.into(),
                            self::types::string(),
                            nullable,
                            metadata("String".into()),
                        )),
                        InstanceType::Array => value.array.to_array_data_type()?.map(|type_| {
                            SchemaField::new(
                                name.into(),
                                SchemaDataType::array(type_),
                                nullable,
                                metadata("Array".into()),
                            )
                        }),
                        InstanceType::Object => Some(SchemaField::new(
                            name.into(),
                            SchemaDataType::r#struct(SchemaTypeStruct::new(
                                value.object.to_data_types(api_version, definitions)?,
                            )),
                            nullable,
                            metadata("Object".into()),
                        )),
                    })
                }

                match self {
                    Schema::Bool(true) => bail!("dynamic object is not supported yet"),
                    Schema::Bool(false) => Ok(None),
                    Schema::Object(value) => {
                        let metadata =
                            |kind| match json!(value.metadata.clone().unwrap_or_default()) {
                                Value::Object(mut metadata) => {
                                    metadata.insert("apiVersion".into(), api_version.clone());
                                    metadata.insert("array".into(), json!(&value.array));
                                    metadata.insert("format".into(), json!(&value.format));
                                    metadata.insert("kind".into(), kind);
                                    metadata.insert("number".into(), json!(&value.number));
                                    metadata.insert("object".into(), json!(&value.object));
                                    metadata.insert("string".into(), json!(&value.string));
                                    metadata.into_iter().collect()
                                }
                                _ => unreachable!("json schema metadata should be Object"),
                            };

                        const REFERENCE_ROOT: &str = "#/definitions/";
                        let instance_type = match value.reference.as_ref() {
                            Some(reference) if reference.starts_with(REFERENCE_ROOT) => {
                                match definitions.get(&reference[REFERENCE_ROOT.len()..]) {
                                    Some(schema) => {
                                        return schema.to_data_type(
                                            api_version,
                                            definitions,
                                            name,
                                            nullable,
                                        );
                                    }
                                    None => bail!("no such json schema reference: {reference:?}"),
                                }
                            }
                            Some(reference) => {
                                bail!("relative json schema reference is not supported yet: {reference:?}")
                            }
                            None => value.instance_type.as_ref(),
                        };

                        let ctx = Context {
                            api_version,
                            definitions,
                            name,
                        };
                        Ok(match instance_type {
                            Some(SingleOrVec::Single(instance_type)) => {
                                parse_instance_type(ctx, value, instance_type, metadata, nullable)?
                            }
                            Some(SingleOrVec::Vec(instance_types)) => match instance_types.len() {
                                0 => None,
                                1 => parse_instance_type(
                                    ctx,
                                    value,
                                    &instance_types[0],
                                    metadata,
                                    nullable,
                                )?,
                                2 => match find_instance_type_none(instance_types) {
                                    Some(index) => parse_instance_type(
                                        ctx,
                                        value,
                                        &instance_types[1 - index],
                                        metadata,
                                        true,
                                    )?,
                                    None => bail!("union object is not supported yet"),
                                },
                                _ => bail!("union object is not supported yet"),
                            },
                            None => None,
                        })
                    }
                }
            }
        }

        trait JsonFieldColumnArray {
            fn to_array_data_type(&self) -> Result<Option<SchemaTypeArray>>;
        }

        impl JsonFieldColumnArray for Option<Box<ArrayValidation>> {
            fn to_array_data_type(&self) -> Result<Option<SchemaTypeArray>> {
                fn parse_instance_type(
                    instance_type: &InstanceType,
                    nullable: bool,
                ) -> Result<Option<SchemaTypeArray>> {
                    Ok(match instance_type {
                        InstanceType::Null => None,
                        InstanceType::Boolean => Some(SchemaTypeArray::new(
                            self::types::boolean().into(),
                            nullable,
                        )),
                        InstanceType::Integer => Some(SchemaTypeArray::new(
                            self::types::integer().into(),
                            nullable,
                        )),
                        InstanceType::Number => {
                            Some(SchemaTypeArray::new(self::types::number().into(), nullable))
                        }
                        InstanceType::String => {
                            Some(SchemaTypeArray::new(self::types::string().into(), nullable))
                        }
                        InstanceType::Array => {
                            bail!("nested array is not supported yet")
                        }
                        InstanceType::Object => {
                            bail!("nested object array is not supported yet")
                        }
                    })
                }

                let nullable = false;
                match self.as_ref().and_then(|value| value.items.as_ref()) {
                    Some(SingleOrVec::Single(value)) => match &**value {
                        Schema::Bool(true) => {
                            bail!("dynamic array is not supported yet")
                        }
                        Schema::Bool(false) => Ok(None),
                        Schema::Object(value) => match &value.instance_type {
                            Some(SingleOrVec::Single(instance_type)) => {
                                parse_instance_type(instance_type, nullable)
                            }
                            Some(SingleOrVec::Vec(instance_types)) => match instance_types.len() {
                                0 => Ok(None),
                                1 => parse_instance_type(&instance_types[0], nullable),
                                2 => match find_instance_type_none(instance_types) {
                                    Some(index) => {
                                        parse_instance_type(&instance_types[1 - index], true)
                                    }
                                    None => bail!("union array is not supported yet"),
                                },
                                _ => bail!("union array is not supported yet"),
                            },
                            None => Ok(None),
                        },
                    },
                    Some(SingleOrVec::Vec(_)) => {
                        bail!("union array is not supported yet")
                    }
                    None => Ok(None),
                }
            }
        }

        trait JsonFieldColumns {
            fn to_data_types(
                &self,
                api_version: &Value,
                definitions: &::schemars::Map<String, Schema>,
            ) -> Result<Vec<SchemaField>>;
        }

        impl JsonFieldColumns for Box<ObjectValidation> {
            fn to_data_types(
                &self,
                api_version: &Value,
                definitions: &schemars::Map<String, Schema>,
            ) -> Result<Vec<SchemaField>> {
                self.properties
                    .iter()
                    .filter_map(|(child_name, child)| {
                        let nullable = !self.required.contains(child_name);
                        child
                            .to_data_type(api_version, definitions, child_name, nullable)
                            .transpose()
                    })
                    .collect()
            }
        }

        impl JsonFieldColumns for Option<Box<ObjectValidation>> {
            fn to_data_types(
                &self,
                api_version: &Value,
                definitions: &schemars::Map<String, Schema>,
            ) -> Result<Vec<SchemaField>> {
                match self {
                    Some(value) => value.to_data_types(api_version, definitions),
                    None => Ok(Default::default()),
                }
            }
        }

        let api_version = json!(self
            .meta_schema
            .as_deref()
            .unwrap_or("http://json-schema.org/"));
        let definitions = &self.definitions;

        self.schema.object.to_data_types(&api_version, definitions)
    }
}

impl FieldColumns for [ModelFieldNativeSpec] {
    fn to_data_types(&self) -> Result<Vec<SchemaField>> {
        struct FieldBuilder {
            name: String,
            type_: FieldBuilderType,
            attributes: ModelFieldAttributeSpec,
            metadata: FieldMetadata,
        }

        impl FieldBuilder {
            fn push<'a>(
                &mut self,
                api_version: &Value,
                mut child_names: impl Iterator<Item = &'a str>,
                name: &'a str,
                field: &'a ModelFieldNativeSpec,
            ) -> Result<()> {
                match &mut self.type_ {
                    FieldBuilderType::Object(children) => match child_names.next() {
                        Some(child_name) => children
                            .entry(name.into())
                            .or_insert(Self {
                                name: name.into(),
                                type_: FieldBuilderType::Object(Default::default()),
                                attributes: field.attribute,
                                metadata: field.to_metadata(api_version),
                            })
                            .push(api_version, child_names, child_name, field),
                        None => match &field.kind {
                            // BEGIN primitive types
                            ModelFieldKindNativeSpec::None {} => Ok(()),
                            ModelFieldKindNativeSpec::Boolean { default: _ } => {
                                children.insert(
                                    name.into(),
                                    Self {
                                        name: name.into(),
                                        type_: FieldBuilderType::Primitive(
                                            FieldBuilderPrimitiveType::Boolean,
                                        ),
                                        attributes: field.attribute,
                                        metadata: field.to_metadata(api_version),
                                    },
                                );
                                Ok(())
                            }
                            ModelFieldKindNativeSpec::Integer {
                                default: _,
                                minimum: _,
                                maximum: _,
                            } => {
                                children.insert(
                                    name.into(),
                                    Self {
                                        name: name.into(),
                                        type_: FieldBuilderType::Primitive(
                                            FieldBuilderPrimitiveType::Integer,
                                        ),
                                        attributes: field.attribute,
                                        metadata: field.to_metadata(api_version),
                                    },
                                );
                                Ok(())
                            }
                            ModelFieldKindNativeSpec::Number {
                                default: _,
                                minimum: _,
                                maximum: _,
                            } => {
                                children.insert(
                                    name.into(),
                                    Self {
                                        name: name.into(),
                                        type_: FieldBuilderType::Primitive(
                                            FieldBuilderPrimitiveType::Number,
                                        ),
                                        attributes: field.attribute,
                                        metadata: field.to_metadata(api_version),
                                    },
                                );
                                Ok(())
                            }
                            ModelFieldKindNativeSpec::String {
                                default: _,
                                kind: _,
                            } => {
                                children.insert(
                                    name.into(),
                                    Self {
                                        name: name.into(),
                                        type_: FieldBuilderType::Primitive(
                                            FieldBuilderPrimitiveType::String,
                                        ),
                                        attributes: field.attribute,
                                        metadata: field.to_metadata(api_version),
                                    },
                                );
                                Ok(())
                            }
                            ModelFieldKindNativeSpec::OneOfStrings {
                                default: _,
                                choices: _,
                            } => {
                                children.insert(
                                    name.into(),
                                    Self {
                                        name: name.into(),
                                        type_: FieldBuilderType::Primitive(
                                            FieldBuilderPrimitiveType::String,
                                        ),
                                        attributes: field.attribute,
                                        metadata: field.to_metadata(api_version),
                                    },
                                );
                                Ok(())
                            }
                            // BEGIN string formats
                            ModelFieldKindNativeSpec::DateTime { default: _ } => {
                                children.insert(
                                    name.into(),
                                    Self {
                                        name: name.into(),
                                        type_: FieldBuilderType::Primitive(
                                            FieldBuilderPrimitiveType::DateTime,
                                        ),
                                        attributes: field.attribute,
                                        metadata: field.to_metadata(api_version),
                                    },
                                );
                                Ok(())
                            }
                            ModelFieldKindNativeSpec::Ip {} => {
                                children.insert(
                                    name.into(),
                                    Self {
                                        name: name.into(),
                                        type_: FieldBuilderType::Primitive(
                                            FieldBuilderPrimitiveType::String,
                                        ),
                                        attributes: field.attribute,
                                        metadata: field.to_metadata(api_version),
                                    },
                                );
                                Ok(())
                            }
                            ModelFieldKindNativeSpec::Uuid {} => {
                                children.insert(
                                    name.into(),
                                    Self {
                                        name: name.into(),
                                        type_: FieldBuilderType::Primitive(
                                            FieldBuilderPrimitiveType::String,
                                        ),
                                        attributes: field.attribute,
                                        metadata: field.to_metadata(api_version),
                                    },
                                );
                                Ok(())
                            }
                            // BEGIN aggregation types
                            ModelFieldKindNativeSpec::StringArray {} => {
                                children.insert(
                                    name.into(),
                                    Self {
                                        name: name.into(),
                                        type_: FieldBuilderType::Array(
                                            FieldBuilderArrayType::Primitive(
                                                FieldBuilderPrimitiveType::String,
                                            ),
                                        ),
                                        attributes: field.attribute,
                                        metadata: field.to_metadata(api_version),
                                    },
                                );
                                Ok(())
                            }
                            ModelFieldKindNativeSpec::Object { children: _, kind } => match kind {
                                ModelFieldKindObjectSpec::Dynamic {} => {
                                    children.insert(
                                        name.into(),
                                        Self {
                                            name: name.into(),
                                            type_: FieldBuilderType::Dynamic,
                                            attributes: field.attribute,
                                            metadata: field.to_metadata(api_version),
                                        },
                                    );
                                    Ok(())
                                }
                                ModelFieldKindObjectSpec::Enumerate { choices: _ }
                                | ModelFieldKindObjectSpec::Static {} => {
                                    children.insert(
                                        name.into(),
                                        Self {
                                            name: name.into(),
                                            type_: FieldBuilderType::Object(Default::default()),
                                            attributes: field.attribute,
                                            metadata: field.to_metadata(api_version),
                                        },
                                    );
                                    Ok(())
                                }
                            },
                            ModelFieldKindNativeSpec::ObjectArray { children: _ } => {
                                children.insert(
                                    name.into(),
                                    Self {
                                        name: name.into(),
                                        type_: FieldBuilderType::Array(
                                            FieldBuilderArrayType::Object,
                                        ),
                                        attributes: field.attribute,
                                        metadata: field.to_metadata(api_version),
                                    },
                                );
                                Ok(())
                            }
                        },
                    },
                    _ => bail!("the parent field should be Object"),
                }
            }

            fn try_into_children(self) -> Result<BTreeMap<String, Self>> {
                match self.type_ {
                    FieldBuilderType::Object(children) => Ok(children),
                    _ => bail!("cannot convert field builder to object"),
                }
            }
        }

        impl TryFrom<FieldBuilder> for SchemaField {
            type Error = Error;

            fn try_from(field: FieldBuilder) -> Result<Self> {
                let FieldBuilder {
                    name,
                    type_,
                    attributes: ModelFieldAttributeSpec { optional: nullable },
                    metadata,
                } = field;

                Ok(Self::new(
                    name,
                    match type_ {
                        FieldBuilderType::Primitive(type_) => type_.into(),
                        FieldBuilderType::Array(type_) => {
                            SchemaDataType::array(SchemaTypeArray::new(
                                Box::new(match type_ {
                                    FieldBuilderArrayType::Primitive(type_) => type_.into(),
                                    FieldBuilderArrayType::Object => {
                                        bail!("object array is not supported yet")
                                    }
                                }),
                                nullable,
                            ))
                        }
                        FieldBuilderType::Object(children) => {
                            SchemaDataType::r#struct(SchemaTypeStruct::new(
                                children
                                    .into_values()
                                    .map(TryInto::try_into)
                                    .collect::<Result<_>>()?,
                            ))
                        }
                        FieldBuilderType::Dynamic => bail!("dynamic array is not supported yet"),
                    },
                    nullable,
                    metadata,
                ))
            }
        }

        enum FieldBuilderType {
            Primitive(FieldBuilderPrimitiveType),
            Array(FieldBuilderArrayType),
            Object(BTreeMap<String, FieldBuilder>),
            Dynamic,
        }

        enum FieldBuilderArrayType {
            Primitive(FieldBuilderPrimitiveType),
            Object,
        }

        enum FieldBuilderPrimitiveType {
            Boolean,
            Integer,
            Number,
            String,
            DateTime,
        }

        impl From<FieldBuilderPrimitiveType> for SchemaDataType {
            fn from(value: FieldBuilderPrimitiveType) -> Self {
                SchemaDataType::primitive(
                    match value {
                        FieldBuilderPrimitiveType::Boolean => "boolean",
                        FieldBuilderPrimitiveType::Integer => "long",
                        FieldBuilderPrimitiveType::Number => "double",
                        FieldBuilderPrimitiveType::String => "string",
                        FieldBuilderPrimitiveType::DateTime => "timestamp",
                    }
                    .into(),
                )
            }
        }

        trait ToFieldMetadata {
            fn to_metadata(&self, api_version: &Value) -> FieldMetadata;
        }

        impl ToFieldMetadata for ModelFieldNativeSpec {
            fn to_metadata(&self, api_version: &Value) -> FieldMetadata {
                match &self.kind {
                    // BEGIN primitive types
                    ModelFieldKindNativeSpec::None {} => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "kind".into() => json!("None"),
                    },
                    ModelFieldKindNativeSpec::Boolean { default } => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "kind".into() => json!("Boolean"),
                        "default".into() => json!(default),
                    },
                    ModelFieldKindNativeSpec::Integer {
                        default,
                        minimum,
                        maximum,
                    } => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "kind".into() => json!("Integer"),
                        "default".into() => json!(default),
                        "minimum".into() => json!(minimum),
                        "maximum".into() => json!(maximum),
                    },
                    ModelFieldKindNativeSpec::Number {
                        default,
                        minimum,
                        maximum,
                    } => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "kind".into() => json!("Number"),
                        "default".into() => json!(default),
                        "minimum".into() => json!(minimum),
                        "maximum".into() => json!(maximum),
                    },
                    ModelFieldKindNativeSpec::String { default, kind } => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "kind".into() => json!("String"),
                        "default".into() => json!(default),
                        "spec".into() => json!(kind),
                    },
                    ModelFieldKindNativeSpec::OneOfStrings { default, choices } => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "kind".into() => json!("OneOfStrings"),
                        "default".into() => json!(default),
                        "choices".into() => json!(choices),
                    },
                    // BEGIN string formats
                    ModelFieldKindNativeSpec::DateTime { default } => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "kind".into() => json!("DateTime"),
                        "default".into() => json!(default),
                    },
                    ModelFieldKindNativeSpec::Ip {} => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "kind".into() => json!("Ip"),
                    },
                    ModelFieldKindNativeSpec::Uuid {} => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "kind".into() => json!("Uuid"),
                    },
                    // BEGIN aggregation types
                    ModelFieldKindNativeSpec::StringArray {} => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "arrayKind".into() => json!("String"),
                        "kind".into() => json!("Array"),
                    },
                    ModelFieldKindNativeSpec::Object { children, kind } => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "kind".into() => json!("Object"),
                        "children".into() => json!(children),
                        "spec".into() => json!(kind),
                    },
                    ModelFieldKindNativeSpec::ObjectArray { children } => hash_map! {
                        "apiVersion".into() => json!(api_version),
                        "arrayKind".into() => json!("Object"),
                        "kind".into() => json!("Array"),
                        "children".into() => json!(children),
                    },
                }
            }
        }

        let api_version = json!(format!(
            "http://{crd_version}",
            crd_version = {
                use kube::Resource;
                ::dash_api::model::ModelCrd::api_version(&())
            },
        ));

        let root = match self.get(0) {
            Some(root) => root,
            None => return Ok(Default::default()),
        };
        let mut root = FieldBuilder {
            name: Default::default(),
            type_: FieldBuilderType::Object(Default::default()),
            attributes: root.attribute,
            metadata: root.to_metadata(&api_version),
        };

        for field in &self[1..] {
            let mut field_child_names = field.name[1..field.name.len() - 1].split('/');
            let field_name = field_child_names
                .next()
                .ok_or_else(|| anyhow!("fields are not ordered"))?;
            root.push(&api_version, field_child_names, field_name, field)?;
        }
        root.try_into_children()
            .and_then(|children| children.into_values().map(TryInto::try_into).collect())
    }
}

impl FieldColumns for Vec<ModelFieldNativeSpec> {
    fn to_data_types(&self) -> Result<Vec<SchemaField>> {
        self.as_slice().to_data_types()
    }
}

pub trait FieldSchema {
    fn to_data_type(&self) -> Result<Option<SchemaDataType>>;
}

impl FieldSchema for DataType {
    fn to_data_type(&self) -> Result<Option<SchemaDataType>> {
        Ok(match self {
            DataType::Null => None,
            DataType::Boolean => Some(self::types::boolean()),
            DataType::Int8 | DataType::UInt8 => Some(SchemaDataType::primitive("byte".into())),
            DataType::Int16 | DataType::UInt16 => Some(SchemaDataType::primitive("short".into())),
            DataType::Int32 | DataType::UInt32 => Some(SchemaDataType::primitive("integer".into())),
            DataType::Int64 | DataType::UInt64 => Some(SchemaDataType::primitive("long".into())),
            DataType::Float32 => Some(SchemaDataType::primitive("float".into())),
            DataType::Float64 => Some(SchemaDataType::primitive("double".into())),
            DataType::Decimal128(_, _) | DataType::Decimal256(_, _) => {
                Some(SchemaDataType::primitive("decimal".into()))
            }
            DataType::Date32 | DataType::Date64 => Some(SchemaDataType::primitive("date".into())),
            DataType::Timestamp(_, _) => Some(self::types::date_time()),
            DataType::Binary | DataType::FixedSizeBinary(_) | DataType::LargeBinary => {
                Some(SchemaDataType::primitive("binary".into()))
            }
            DataType::Utf8 | DataType::LargeUtf8 => Some(self::types::string()),
            type_ => bail!("unsupportd data type: {type_:?}"),
            // DataType::Float16 => todo!(),
            // DataType::Time32(_) => todo!(),
            // DataType::Time64(_) => todo!(),
            // DataType::Duration(_) => todo!(),
            // DataType::Interval(_) => todo!(),
            // DataType::List(_) => todo!(),
            // DataType::FixedSizeList(_, _) => todo!(),
            // DataType::LargeList(_) => todo!(),
            // DataType::Struct(_) => todo!(),
            // DataType::Union(_, _) => todo!(),
            // DataType::Dictionary(_, _) => todo!(),
            // DataType::Map(_, _) => todo!(),
            // DataType::RunEndEncoded(_, _) => todo!(),
        })
    }
}

mod types {
    use deltalake::SchemaDataType;

    pub(super) fn boolean() -> SchemaDataType {
        SchemaDataType::primitive("boolean".into())
    }

    pub(super) fn integer() -> SchemaDataType {
        SchemaDataType::primitive("long".into())
    }

    pub(super) fn number() -> SchemaDataType {
        SchemaDataType::primitive("double".into())
    }

    pub(super) fn string() -> SchemaDataType {
        SchemaDataType::primitive("string".into())
    }

    pub(super) fn date_time() -> SchemaDataType {
        SchemaDataType::primitive("timestamp".into())
    }
}

type FieldMetadata = ::std::collections::HashMap<String, Value>;
