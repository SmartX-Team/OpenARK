use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fmt,
    net::IpAddr,
    str::{FromStr, Split},
};

use dash_api::model::{
    ModelFieldDateTimeDefaultType, ModelFieldKindNativeSpec, ModelFieldNativeSpec,
    ModelFieldsNativeSpec,
};
use ipis::core::{
    anyhow::{anyhow, bail, Error, Result},
    chrono::{DateTime, Utc},
    uuid::Uuid,
};
use kiss_api::serde_json::{Map, Value};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputTemplate {
    basemap: BTreeMap<String, ModelFieldNativeSpec>,
    map: Value,
}

impl InputTemplate {
    pub fn new_empty(fields: ModelFieldsNativeSpec) -> Self {
        Self {
            basemap: fields
                .into_iter()
                .map(|field| (field.name.clone(), field))
                .collect(),
            map: Default::default(),
        }
    }

    pub fn update_fields(&mut self, inputs: Vec<InputField>) -> Result<()> {
        inputs
            .into_iter()
            .try_for_each(|input| self.update_field(input))
    }

    pub fn update_field(&mut self, input: InputField) -> Result<()> {
        let InputField { name, value } = input;

        let (base_field, field) = self.get_field(&name)?;

        match &base_field.kind {
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
                let value_i64: i64 = value.parse()?;
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
                let value_f64: f64 = value.parse()?;
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
            ModelFieldKindNativeSpec::String { default: _ } => {
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
            ModelFieldKindNativeSpec::Object { .. }
            | ModelFieldKindNativeSpec::ObjectArray { .. } => {
                let type_ = base_field.kind.to_type();
                let value = &value;
                bail!("cannot set {type_} type to {value:?}: {name:?}")
            }
        }
    }

    fn get_field(&mut self, name: &str) -> Result<(&ModelFieldNativeSpec, &mut Value)> {
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
                            let type_ = base_field.kind.to_type();
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
        Ok((base_field, field))
    }

    fn fill_default_value(&mut self, name: &str, is_atom: bool) -> Result<()> {
        let (base_field, field) = self.get_field(name)?;

        fn assert_optional(name: &str, base_field: &ModelFieldNativeSpec) -> Result<()> {
            if base_field.optional {
                Ok(())
            } else {
                let type_ = base_field.kind.to_type();
                bail!("missing {type_} value: {name:?}")
            }
        }

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
                        None => assert_optional(name, base_field),
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
                        None => assert_optional(name, base_field),
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
                        None => assert_optional(name, base_field),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::String { default } => {
                if field.is_null() {
                    match default {
                        Some(default) => {
                            *field = Value::String(default.clone());
                            Ok(())
                        }
                        None => assert_optional(name, base_field),
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
                        None => assert_optional(name, base_field),
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
                        None => assert_optional(name, base_field),
                    }
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::Ip {} => {
                if field.is_null() {
                    assert_optional(name, base_field)
                } else {
                    Ok(())
                }
            }
            ModelFieldKindNativeSpec::Uuid {} => {
                if field.is_null() {
                    assert_optional(name, base_field)
                } else {
                    Ok(())
                }
            }
            // BEGIN aggregation types
            ModelFieldKindNativeSpec::Object {
                children,
                dynamic: _,
            } => {
                *field = Value::Object(Default::default());

                for child in crate::imp::get_children_names(children) {
                    self.fill_default_value(&format!("{name}{child}/"), true)?;
                }
                Ok(())
            }
            ModelFieldKindNativeSpec::ObjectArray { children } => {
                if is_atom {
                    *field = Value::Array(Default::default());

                    if let Some(children) = field.as_array() {
                        let children = 0..children.len();
                        for child in children {
                            self.fill_default_value(&format!("{name}{child}/"), false)?;
                        }
                    }
                    Ok(())
                } else {
                    *field = Value::Object(Default::default());

                    for child in crate::imp::get_children_names(children) {
                        self.fill_default_value(&format!("{name}{child}/"), true)?;
                    }
                    Ok(())
                }
            }
        }
    }

    pub fn finalize(mut self) -> Result<Value> {
        self.fill_default_value("/", true).map(|()| self.map)
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputField {
    pub name: String,
    pub value: String,
}

impl FromStr for InputField {
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

enum CursorEntry<'a> {
    EnterArray { basename: String, index: usize },
    EnterObject { basename: String, child: &'a str },
}
