use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fmt,
    net::IpAddr,
    str::{FromStr, Split},
};

use actix_web::dev::ResourcePath;
use anyhow::{anyhow, bail, Error, Result};
use async_recursion::async_recursion;
use chrono::{DateTime, Utc};
use dash_api::model::{
    Integer, ModelFieldDateTimeDefaultType, ModelFieldKindNativeSpec, ModelFieldNativeSpec,
    ModelFieldSpec, ModelFieldsNativeSpec, ModelFieldsSpec, Number,
};
use inflector::Inflector;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::storage::StorageClient;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputTemplate {
    basemap: BTreeMap<String, InputModelFieldSpec>,
    map: Value,
}

impl InputTemplate {
    pub fn new_empty(original: &ModelFieldsSpec, parsed: ModelFieldsNativeSpec) -> Self {
        Self {
            basemap: parsed
                .into_iter()
                .map(|parsed| {
                    (
                        parsed.name.clone(),
                        InputModelFieldSpec {
                            original: original
                                .iter()
                                .find(|original| original.name == parsed.name)
                                .cloned(),
                            parsed,
                        },
                    )
                })
                .collect(),
            map: Default::default(),
        }
    }

    pub async fn update_field_string(
        &mut self,
        storage: &StorageClient<'_, '_>,
        input: InputFieldString,
    ) -> Result<()> {
        self.update_field_string_impl(storage, input, false).await
    }

    async fn update_field_string_impl(
        &mut self,
        storage: &StorageClient<'_, '_>,
        input: InputFieldString,
        optional: bool,
    ) -> Result<()> {
        let InputField { name, value } = input;

        let (base_field, field) = self.get_field(&name)?;
        let optional = optional || base_field.parsed.attribute.optional;

        match &base_field.parsed.kind {
            // BEGIN primitive types
            ModelFieldKindNativeSpec::None {} => {
                *field = Value::Null;
                Ok(())
            }
            ModelFieldKindNativeSpec::Boolean { default: _ } => {
                *field = Value::Bool(value.parse()?);
                Ok(())
            }
            ModelFieldKindNativeSpec::Integer {
                default: _,
                minimum,
                maximum,
            } => {
                let value_i64: Integer = value.parse()?;
                assert_cmp(
                    &name,
                    &value_i64,
                    "minimum",
                    minimum,
                    "greater",
                    Ordering::Greater,
                )?;
                assert_cmp(
                    &name,
                    &value_i64,
                    "maximum",
                    maximum,
                    "less",
                    Ordering::Less,
                )?;

                *field = Value::Number(value_i64.into());
                Ok(())
            }
            ModelFieldKindNativeSpec::Number {
                default: _,
                minimum,
                maximum,
            } => {
                let value_f64: Number = value.parse()?;
                assert_cmp(
                    &name,
                    &value_f64,
                    "minimum",
                    minimum,
                    "greater",
                    Ordering::Greater,
                )?;
                assert_cmp(
                    &name,
                    &value_f64,
                    "maximum",
                    maximum,
                    "less",
                    Ordering::Less,
                )?;

                *field = Value::Number(value.parse()?);
                Ok(())
            }
            ModelFieldKindNativeSpec::String { default: _, kind } => {
                crate::imp::assert_string(&name, &value, kind)?;
                *field = Value::String(value);
                Ok(())
            }
            ModelFieldKindNativeSpec::OneOfStrings {
                default: _,
                choices,
            } => {
                crate::imp::assert_contains(&name, "choices", choices, "value", Some(&value))?;
                *field = Value::String(value);
                Ok(())
            }
            // BEGIN string formats
            ModelFieldKindNativeSpec::DateTime { default: _ } => {
                let _: DateTime<Utc> = crate::imp::assert_type(&name, &value)?;
                *field = Value::String(value);
                Ok(())
            }
            ModelFieldKindNativeSpec::Ip {} => {
                let _: IpAddr = crate::imp::assert_type(&name, &value)?;
                *field = Value::String(value);
                Ok(())
            }
            ModelFieldKindNativeSpec::Uuid {} => {
                let _: Uuid = crate::imp::assert_type(&name, &value)?;
                *field = Value::String(value);
                Ok(())
            }
            // BEGIN aggregation types
            ModelFieldKindNativeSpec::StringArray { .. } => {
                let input = InputFieldValue {
                    name,
                    value: Value::String(value),
                };
                self.update_field_value_impl(storage, input, optional).await
            }
            ModelFieldKindNativeSpec::Object { .. } => {
                let input = InputFieldValue {
                    name,
                    value: storage
                        .get_by_field(base_field.original.as_ref(), &value)
                        .await?,
                };
                self.update_field_value_impl(storage, input, optional).await
            }
            ModelFieldKindNativeSpec::ObjectArray { .. } => {
                assert_optional(&name, &value, &base_field.parsed, optional)
            }
        }
    }

    pub async fn update_field_value(
        &mut self,
        storage: &StorageClient<'_, '_>,
        input: InputFieldValue,
    ) -> Result<()> {
        self.update_field_value_impl(storage, input, false).await
    }

    #[async_recursion]
    async fn update_field_value_impl(
        &mut self,
        storage: &StorageClient<'_, '_>,
        input: InputFieldValue,
        optional: bool,
    ) -> Result<()> {
        let InputField { name, value } = input;

        let (base_field, field) = self.get_field(&name)?;
        let optional = optional || base_field.parsed.attribute.optional;

        match &base_field.parsed.kind {
            // BEGIN primitive types
            ModelFieldKindNativeSpec::None {} => {
                *field = Value::Null;
                Ok(())
            }
            ModelFieldKindNativeSpec::Boolean { default: _ } => {
                if value.is_boolean() {
                    *field = value;
                    Ok(())
                } else {
                    assert_optional(&name, &value, &base_field.parsed, optional)
                }
            }
            ModelFieldKindNativeSpec::Integer {
                default: _,
                minimum,
                maximum,
            } => match value.as_i64() {
                Some(value_number) => {
                    assert_cmp(
                        &name,
                        &value_number,
                        "minimum",
                        minimum,
                        "greater",
                        Ordering::Greater,
                    )?;
                    assert_cmp(
                        &name,
                        &value_number,
                        "maximum",
                        maximum,
                        "less",
                        Ordering::Less,
                    )?;

                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, &base_field.parsed, optional),
            },
            ModelFieldKindNativeSpec::Number {
                default: _,
                minimum,
                maximum,
            } => match value.as_f64().map(Into::into) {
                Some(value_number) => {
                    assert_cmp(
                        &name,
                        &value_number,
                        "minimum",
                        minimum,
                        "greater",
                        Ordering::Greater,
                    )?;
                    assert_cmp(
                        &name,
                        &value_number,
                        "maximum",
                        maximum,
                        "less",
                        Ordering::Less,
                    )?;

                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, &base_field.parsed, optional),
            },
            ModelFieldKindNativeSpec::String { default: _, kind } => match value.as_str() {
                Some(value_str) => {
                    crate::imp::assert_string(&name, value_str, kind)?;
                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, &base_field.parsed, optional),
            },
            ModelFieldKindNativeSpec::OneOfStrings {
                default: _,
                choices,
            } => match value.as_str() {
                Some(value_string) => {
                    crate::imp::assert_contains(
                        &name,
                        "choices",
                        choices,
                        "value",
                        Some(value_string),
                    )?;
                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, &base_field.parsed, optional),
            },
            // BEGIN string formats
            ModelFieldKindNativeSpec::DateTime { default: _ } => match value.as_str() {
                Some(value_string) => {
                    let _: DateTime<Utc> = crate::imp::assert_type(&name, value_string)?;
                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, &base_field.parsed, optional),
            },
            ModelFieldKindNativeSpec::Ip {} => match value.as_str() {
                Some(value_string) => {
                    let _: IpAddr = crate::imp::assert_type(&name, value_string)?;
                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, &base_field.parsed, optional),
            },
            ModelFieldKindNativeSpec::Uuid {} => match value.as_str() {
                Some(value_string) => {
                    let _: Uuid = crate::imp::assert_type(&name, value_string)?;
                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, &base_field.parsed, optional),
            },
            // BEGIN aggregation types
            ModelFieldKindNativeSpec::StringArray {} => match value {
                Value::String(value) => {
                    *field = Value::Array(
                        value
                            .split(',')
                            .map(|value| Value::String(value.into()))
                            .collect(),
                    );
                    Ok(())
                }
                Value::Array(children) => {
                    *field = Value::Array(
                        children
                            .into_iter()
                            .filter_map(|value| match value {
                                Value::String(value) => Some(Value::String(value)),
                                _ => None,
                            })
                            .collect(),
                    );
                    Ok(())
                }
                value => assert_optional(&name, &value, &base_field.parsed, optional),
            },
            ModelFieldKindNativeSpec::Object {
                children: _,
                dynamic,
            } => match value {
                Value::String(ref_name) => {
                    let input = InputFieldValue {
                        name,
                        value: storage
                            .get_by_field(base_field.original.as_ref(), &ref_name)
                            .await?,
                    };
                    self.update_field_value_impl(storage, input, optional).await
                }
                Value::Object(children) => {
                    if *dynamic {
                        *field = Value::Object(children);
                        Ok(())
                    } else {
                        for (child, value) in children.into_iter() {
                            let child = InputField::sub_object(&name, &child, value);
                            self.update_field_value_impl(storage, child, optional)
                                .await?;
                        }
                        Ok(())
                    }
                }
                value => assert_optional(&name, &value, &base_field.parsed, optional),
            },
            ModelFieldKindNativeSpec::ObjectArray { .. } => match value {
                Value::Array(children) => {
                    for (index, value) in children.into_iter().enumerate() {
                        let child = InputField::sub_array(&name, index, value);
                        self.update_field_value_impl(storage, child, optional)
                            .await?;
                    }
                    Ok(())
                }
                value => assert_optional(&name, &value, &base_field.parsed, optional),
            },
        }
    }

    fn get_field(&mut self, name: &str) -> Result<(&InputModelFieldSpec, &mut Value)> {
        let mut base_field = match self.basemap.get("/") {
            Some(field) => field,
            None => bail!("no root field"),
        };
        let mut field = &mut self.map;

        for entry in CursorIterator::from_name(name) {
            field = match entry {
                CursorEntry::EnterArray { basename, index } => {
                    base_field = match self.basemap.get(&basename) {
                        Some(field) => field,
                        None => bail!("no such Array field: {name:?}"),
                    };

                    match field {
                        Value::Null => {
                            *field = Value::Array(vec![Default::default(); index]);
                            &mut field[index]
                        }
                        Value::Array(children) => {
                            if children.len() <= index {
                                children.resize(index + 1, Default::default());
                            }
                            &mut children[index]
                        }
                        _ => {
                            let type_ = base_field.parsed.kind.to_type();
                            bail!("cannot access to {type_} by Array index {index:?}: {name:?}")
                        }
                    }
                }
                CursorEntry::EnterObject { basename, child } => {
                    if child.is_empty() {
                        field
                    } else {
                        base_field = match self.basemap.get(&basename) {
                            Some(field) => field,
                            None => bail!("no such Object field: {name:?}"),
                        };

                        match field {
                            Value::Null => {
                                let mut children: Map<_, _> = Default::default();
                                children.insert(child.to_string(), Default::default());

                                *field = Value::Object(children);
                                &mut field[child]
                            }
                            Value::Object(children) => {
                                children.entry(child).or_insert(Default::default())
                            }
                            _ => {
                                let type_ = base_field.parsed.kind.to_type();
                                bail!(
                                    "cannot access to {type_} by Object field {child:?}: {name:?}"
                                )
                            }
                        }
                    }
                }
            }
        }
        Ok((base_field, field))
    }

    fn fill_default_value(&mut self, name: &str, optional: bool, is_atom: bool) -> Result<()> {
        let (base_field, field) = self.get_field(name)?;
        let optional = optional || base_field.parsed.attribute.optional;

        match &base_field.parsed.kind {
            // BEGIN primitive types
            ModelFieldKindNativeSpec::None {} => {
                *field = Value::Null;
                Ok(())
            }
            ModelFieldKindNativeSpec::Boolean { default } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::Bool(*default);
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, &base_field.parsed),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::Integer {
                default,
                minimum: _,
                maximum: _,
            } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::Number((*default).into());
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, &base_field.parsed),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::Number {
                default,
                minimum: _,
                maximum: _,
            } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::Number(default.to_string().parse()?);
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, &base_field.parsed),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::String { default, kind: _ } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::String(default.clone());
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, &base_field.parsed),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::OneOfStrings {
                default,
                choices: _,
            } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::String(default.clone());
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, &base_field.parsed),
                    }
                } else {
                    Ok(())
                }
            }
            // BEGIN string formats
            ModelFieldKindNativeSpec::DateTime { default } => {
                if field.is_null() {
                    match default {
                        Some(ModelFieldDateTimeDefaultType::Now) => {
                            *field = Value::String(Utc::now().to_rfc3339());
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, &base_field.parsed),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::Ip {} => {
                if field.is_null() {
                    assert_fill_optional(name, optional, &base_field.parsed)
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::Uuid {} => {
                if field.is_null() {
                    assert_fill_optional(name, optional, &base_field.parsed)
                } else {
                    Ok(())
                }
            }
            // BEGIN aggregation types
            ModelFieldKindNativeSpec::StringArray {} => {
                if field.is_null() {
                    *field = Value::Array(Default::default());
                }
                Ok(())
            }
            ModelFieldKindNativeSpec::Object {
                children,
                dynamic: _,
            } => {
                if field.is_null() {
                    *field = Value::Object(Default::default());
                }

                for child in crate::imp::get_children_names(children) {
                    self.fill_default_value(&format!("{name}{child}/"), optional, true)?;
                }
                Ok(())
            }
            ModelFieldKindNativeSpec::ObjectArray { children } => {
                if is_atom {
                    if field.is_null() {
                        *field = Value::Array(Default::default());
                    }

                    if let Some(children) = field.as_array() {
                        let children = 0..children.len();
                        for child in children {
                            self.fill_default_value(&format!("{name}{child}/"), optional, false)?;
                        }
                    }
                    Ok(())
                } else {
                    if field.is_null() {
                        *field = Value::Object(Default::default());
                    }

                    for child in crate::imp::get_children_names(children) {
                        self.fill_default_value(&format!("{name}{child}/"), optional, true)?;
                    }
                    Ok(())
                }
            }
        }
    }

    pub fn finalize(mut self) -> Result<Value> {
        self.fill_default_value("/", false, true).map(|()| self.map)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InputModelFieldSpec {
    original: Option<ModelFieldSpec>,
    parsed: ModelFieldNativeSpec,
}

pub struct ItemTemplate<'a> {
    basemap: BTreeMap<&'a str, &'a ModelFieldNativeSpec>,
    map: Value,
}

impl<'a> ItemTemplate<'a> {
    pub fn new_empty(parsed: &'a ModelFieldsNativeSpec) -> Self {
        Self {
            basemap: parsed
                .iter()
                .map(|parsed| (parsed.name.as_str(), parsed))
                .collect(),
            map: Default::default(),
        }
    }

    pub fn update_field_value(&mut self, input: InputFieldValue) -> Result<()> {
        self.update_field_value_impl(input, false)
    }

    fn update_field_value_impl(&mut self, input: InputFieldValue, optional: bool) -> Result<()> {
        let InputField { name, value } = input;

        let (base_field, field) = match self.try_get_field(&name)? {
            Some((base_field, field)) => (base_field, field),
            None => return Ok(()),
        };
        let optional = optional || base_field.attribute.optional;

        match &base_field.kind {
            // BEGIN primitive types
            ModelFieldKindNativeSpec::None {} => {
                *field = Value::Null;
                Ok(())
            }
            ModelFieldKindNativeSpec::Boolean { default: _ } => {
                if value.is_boolean() {
                    *field = value;
                    Ok(())
                } else {
                    assert_optional(&name, &value, base_field, optional)
                }
            }
            ModelFieldKindNativeSpec::Integer {
                default: _,
                minimum,
                maximum,
            } => match value.as_i64() {
                Some(value_number) => {
                    assert_cmp(
                        &name,
                        &value_number,
                        "minimum",
                        minimum,
                        "greater",
                        Ordering::Greater,
                    )?;
                    assert_cmp(
                        &name,
                        &value_number,
                        "maximum",
                        maximum,
                        "less",
                        Ordering::Less,
                    )?;

                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, base_field, optional),
            },
            ModelFieldKindNativeSpec::Number {
                default: _,
                minimum,
                maximum,
            } => match value.as_f64().map(Into::into) {
                Some(value_number) => {
                    assert_cmp(
                        &name,
                        &value_number,
                        "minimum",
                        minimum,
                        "greater",
                        Ordering::Greater,
                    )?;
                    assert_cmp(
                        &name,
                        &value_number,
                        "maximum",
                        maximum,
                        "less",
                        Ordering::Less,
                    )?;

                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, base_field, optional),
            },
            ModelFieldKindNativeSpec::String { default: _, kind } => match value.as_str() {
                Some(value_str) => {
                    crate::imp::assert_string(&name, value_str, kind)?;
                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, base_field, optional),
            },
            ModelFieldKindNativeSpec::OneOfStrings {
                default: _,
                choices,
            } => match value.as_str() {
                Some(value_string) => {
                    crate::imp::assert_contains(
                        &name,
                        "choices",
                        choices,
                        "value",
                        Some(value_string),
                    )?;
                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, base_field, optional),
            },
            // BEGIN string formats
            ModelFieldKindNativeSpec::DateTime { default: _ } => match value.as_str() {
                Some(value_string) => {
                    let _: DateTime<Utc> = crate::imp::assert_type(&name, value_string)?;
                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, base_field, optional),
            },
            ModelFieldKindNativeSpec::Ip {} => match value.as_str() {
                Some(value_string) => {
                    let _: IpAddr = crate::imp::assert_type(&name, value_string)?;
                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, base_field, optional),
            },
            ModelFieldKindNativeSpec::Uuid {} => match value.as_str() {
                Some(value_string) => {
                    let _: Uuid = crate::imp::assert_type(&name, value_string)?;
                    *field = value;
                    Ok(())
                }
                None => assert_optional(&name, &value, base_field, optional),
            },
            // BEGIN aggregation types
            ModelFieldKindNativeSpec::StringArray {} => match value {
                Value::Array(children) => {
                    *field = Value::Array(
                        children
                            .into_iter()
                            .filter_map(|value| match value {
                                Value::String(value) => Some(Value::String(value)),
                                _ => None,
                            })
                            .collect(),
                    );
                    Ok(())
                }
                value => assert_optional(&name, &value, base_field, optional),
            },
            ModelFieldKindNativeSpec::Object {
                children: _,
                dynamic,
            } => match value {
                Value::Object(children) => {
                    if *dynamic {
                        *field = Value::Object(children);
                        Ok(())
                    } else {
                        for (child, value) in children.into_iter() {
                            let child = InputField::sub_object(&name, &child, value);
                            self.update_field_value_impl(child, optional)?;
                        }
                        Ok(())
                    }
                }
                value => assert_optional(&name, &value, base_field, optional),
            },
            ModelFieldKindNativeSpec::ObjectArray { .. } => match value {
                Value::Array(children) => {
                    for (index, value) in children.into_iter().enumerate() {
                        let child = InputField::sub_array(&name, index, value);
                        self.update_field_value_impl(child, optional)?;
                    }
                    Ok(())
                }
                value => assert_optional(&name, &value, base_field, optional),
            },
        }
    }

    fn get_field(&mut self, name: &str) -> Result<(&ModelFieldNativeSpec, &mut Value)> {
        self.try_get_field(name)
            .and_then(|result| result.ok_or_else(|| anyhow!("no such field: {name:?}")))
    }

    fn try_get_field(&mut self, name: &str) -> Result<Option<(&ModelFieldNativeSpec, &mut Value)>> {
        let mut base_field = match self.basemap.get("/") {
            Some(field) => field,
            None => bail!("no root field"),
        };
        let mut field = &mut self.map;

        for entry in CursorIterator::from_name(name) {
            field = match entry {
                CursorEntry::EnterArray { basename, index } => {
                    base_field = match self.basemap.get(basename.as_str()) {
                        Some(field) => field,
                        None => bail!("no such Array field: {name:?}"),
                    };

                    match field {
                        Value::Null => {
                            *field = Value::Array(vec![Default::default(); index]);
                            &mut field[index]
                        }
                        Value::Array(children) => {
                            if children.len() <= index {
                                children.resize(index + 1, Default::default());
                            }
                            &mut children[index]
                        }
                        _ => {
                            let type_ = base_field.kind.to_type();
                            bail!("cannot access to {type_} by Array index {index:?}: {name:?}")
                        }
                    }
                }
                CursorEntry::EnterObject { basename, child } => {
                    if child.is_empty() {
                        field
                    } else {
                        base_field = match self.basemap.get(basename.as_str()) {
                            Some(field) => field,
                            None => return Ok(None),
                        };

                        match field {
                            Value::Null => {
                                let mut children: Map<_, _> = Default::default();
                                children.insert(child.to_string(), Default::default());

                                *field = Value::Object(children);
                                &mut field[child]
                            }
                            Value::Object(children) => {
                                children.entry(child).or_insert(Default::default())
                            }
                            _ => {
                                let type_ = base_field.kind.to_type();
                                bail!(
                                    "cannot access to {type_} by Object field {child:?}: {name:?}"
                                )
                            }
                        }
                    }
                }
            }
        }
        Ok(Some((base_field, field)))
    }

    fn fill_default_value(&mut self, name: &str, optional: bool, is_atom: bool) -> Result<()> {
        let (base_field, field) = self.get_field(name)?;
        let optional = optional || base_field.attribute.optional;

        match &base_field.kind {
            // BEGIN primitive types
            ModelFieldKindNativeSpec::None {} => {
                *field = Value::Null;
                Ok(())
            }
            ModelFieldKindNativeSpec::Boolean { default } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::Bool(*default);
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, base_field),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::Integer {
                default,
                minimum: _,
                maximum: _,
            } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::Number((*default).into());
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, base_field),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::Number {
                default,
                minimum: _,
                maximum: _,
            } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::Number(default.to_string().parse()?);
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, base_field),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::String { default, kind: _ } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::String(default.clone());
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, base_field),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::OneOfStrings {
                default,
                choices: _,
            } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::String(default.clone());
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, base_field),
                    }
                } else {
                    Ok(())
                }
            }
            // BEGIN string formats
            ModelFieldKindNativeSpec::DateTime { default } => {
                if field.is_null() {
                    match default {
                        Some(ModelFieldDateTimeDefaultType::Now) => {
                            *field = Value::String(Utc::now().to_rfc3339());
                            Ok(())
                        }
                        None => assert_fill_optional(name, optional, base_field),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::Ip {} => {
                if field.is_null() {
                    assert_fill_optional(name, optional, base_field)
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::Uuid {} => {
                if field.is_null() {
                    assert_fill_optional(name, optional, base_field)
                } else {
                    Ok(())
                }
            }
            // BEGIN aggregation types
            ModelFieldKindNativeSpec::StringArray {} => {
                if field.is_null() {
                    *field = Value::Array(Default::default());
                }
                Ok(())
            }
            ModelFieldKindNativeSpec::Object {
                children,
                dynamic: _,
            } => {
                if field.is_null() {
                    *field = Value::Object(Default::default());
                }

                for child in crate::imp::get_children_names(children) {
                    self.fill_default_value(&format!("{name}{child}/"), optional, true)?;
                }
                Ok(())
            }
            ModelFieldKindNativeSpec::ObjectArray { children } => {
                if is_atom {
                    if field.is_null() {
                        *field = Value::Array(Default::default());
                    }

                    if let Some(children) = field.as_array() {
                        let children = 0..children.len();
                        for child in children {
                            self.fill_default_value(&format!("{name}{child}/"), optional, false)?;
                        }
                    }
                    Ok(())
                } else {
                    if field.is_null() {
                        *field = Value::Object(Default::default());
                    }

                    for child in crate::imp::get_children_names(children) {
                        self.fill_default_value(&format!("{name}{child}/"), optional, true)?;
                    }
                    Ok(())
                }
            }
        }
    }

    pub fn finalize(mut self) -> Result<Value> {
        self.fill_default_value("/", false, true).map(|()| self.map)
    }
}

pub type InputFieldString = InputField<String>;
pub type InputFieldValue = InputField<Value>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputField<Value> {
    pub name: String,
    pub value: Value,
}

impl FromStr for InputFieldString {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let re = Regex::new(crate::name::RE_SET)?;
        re.captures(s)
            .and_then(|captures| captures.iter().flatten().last())
            .map(|m| Self {
                name: s[..m.start()].to_string(),
                value: s[m.start()..m.end()].to_string(),
            })
            .ok_or_else(|| anyhow!("field name is invalid: {s} {s:?}"))
    }
}

impl<Value> InputField<Value> {
    fn sub_array(parent: &str, index: usize, value: Value) -> Self {
        Self {
            name: format!("{parent}{index}/"),
            value,
        }
    }

    fn sub_object(parent: &str, child: &str, value: Value) -> Self {
        Self {
            name: format!("{parent}{}/", child.to_snake_case()),
            value,
        }
    }
}

struct CursorIterator<'a> {
    basename: String,
    split: Split<'a, char>,
}

impl<'a> CursorIterator<'a> {
    fn from_name(name: &'a str) -> Self {
        CursorIterator {
            basename: '/'.to_string(),
            split: name.split('/'),
        }
    }
}

impl<'a> Iterator for CursorIterator<'a> {
    type Item = CursorEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.split.next().map(|child| match child.parse::<usize>() {
            Ok(index) => CursorEntry::EnterArray {
                basename: self.basename.clone(),
                index,
            },
            Err(_) => {
                if !child.is_empty() {
                    self.basename = format!("{}{child}/", self.basename);
                };
                CursorEntry::EnterObject {
                    basename: self.basename.clone(),
                    child,
                }
            }
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct Name(pub String);

impl FromStr for Name {
    type Err = Error;

    fn from_str(name: &str) -> std::result::Result<Self, Self::Err> {
        let re = Regex::new(crate::name::RE_CHILD)?;
        if re.is_match(name) {
            Ok(Self(name.to_string()))
        } else {
            bail!("name is invalid: {name} {name:?}")
        }
    }
}

impl<'de> Deserialize<'de> for Name {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)
            .and_then(|name| name.parse().map_err(::serde::de::Error::custom))
    }
}

// impl FromRequest for ModelName {
//     type Error = ::actix_web::Error;
//     type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

//     fn from_request(
//         req: &actix_web::HttpRequest,
//         payload: &mut actix_web::dev::Payload,
//     ) -> Self::Future {
//         let req = req.clone();
//         Box::pin(async move {
//             let name = String::from_request(&req, payload).await?;
//             todo!()
//         })
//     }
// }

impl ResourcePath for Name {
    fn path(&self) -> &str {
        &self.0
    }
}

enum CursorEntry<'a> {
    EnterArray { basename: String, index: usize },
    EnterObject { basename: String, child: &'a str },
}

trait IsDefault {
    fn is_default(&self) -> bool;
}

impl<T> IsDefault for &T
where
    T: IsDefault,
{
    fn is_default(&self) -> bool {
        (**self).is_default()
    }
}

impl IsDefault for String {
    fn is_default(&self) -> bool {
        self.is_empty()
    }
}

impl IsDefault for Value {
    fn is_default(&self) -> bool {
        self.is_null()
    }
}

fn assert_cmp<T>(
    name: &str,
    subject: &T,
    object_label: &str,
    object: &Option<T>,
    ordering_label: &str,
    ordering: Ordering,
) -> Result<()>
where
    T: Copy + fmt::Debug + PartialOrd,
{
    match object {
        Some(object) =>  match subject.partial_cmp(object) {
            Some(Ordering::Equal) => Ok(()),
            Some(result) if result == ordering => Ok(()),
            _ => bail!("value {subject:?} should be {ordering_label} than {object_label} value {object:?}: {name:?}"),
        }
        _ => Ok(()),
    }
}

fn assert_fill_optional(name: &str, optional: bool, spec: &ModelFieldNativeSpec) -> Result<()> {
    if optional {
        Ok(())
    } else {
        let type_ = spec.kind.to_type();
        bail!("missing {type_} value: {name:?}")
    }
}

fn assert_optional<Value>(
    name: &str,
    value: Value,
    spec: &ModelFieldNativeSpec,
    optional: bool,
) -> Result<()>
where
    Value: fmt::Debug + IsDefault,
{
    if optional && value.is_default() {
        Ok(())
    } else {
        let type_ = spec.kind.to_type();
        bail!("type mismatch; expected {type_}, but given {value:?}: {name:?}")
    }
}
