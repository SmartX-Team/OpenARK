use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fmt,
    net::IpAddr,
    str::{FromStr, Split},
};

use dash_api::model::{ModelFieldKindNativeSpec, ModelFieldNativeSpec, ModelFieldsNativeSpec};
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
        let name = &input.name;

        let (base_field, field) = {
            let mut base_field = match self.basemap.get("/") {
                Some(field) => field,
                None => bail!("no root field"),
            };
            let mut field = &mut self.map;

            for entry in input.cursor() {
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
            (base_field, field)
        };

        match &base_field.kind {
            // BEGIN primitive types
            ModelFieldKindNativeSpec::None {} => {
                *field = Value::Null;
                Ok(())
            }
            ModelFieldKindNativeSpec::Boolean { default: _ } => {
                *field = Value::Bool(input.value.parse()?);
                Ok(())
            }
            ModelFieldKindNativeSpec::Integer {
                default: _,
                minimum,
                maximum,
            } => {
                let value: i64 = input.value.parse()?;
                assert_cmp(
                    name,
                    &value,
                    "minimum",
                    minimum,
                    "greater",
                    Ordering::Greater,
                )?;
                assert_cmp(name, &value, "maximum", maximum, "less", Ordering::Less)?;

                *field = Value::Number(value.into());
                Ok(())
            }
            ModelFieldKindNativeSpec::Number {
                default: _,
                minimum,
                maximum,
            } => {
                let value: f64 = input.value.parse()?;
                assert_cmp(
                    name,
                    &value,
                    "minimum",
                    minimum,
                    "greater",
                    Ordering::Greater,
                )?;
                assert_cmp(name, &value, "maximum", maximum, "less", Ordering::Less)?;

                *field = Value::Number(input.value.parse()?);
                Ok(())
            }
            ModelFieldKindNativeSpec::String { default: _ } => {
                *field = Value::String(input.value);
                Ok(())
            }
            ModelFieldKindNativeSpec::OneOfStrings {
                default: _,
                choices,
            } => {
                crate::imp::assert_contains(name, "choices", choices, "value", Some(&input.value))?;
                *field = Value::String(input.value);
                Ok(())
            }
            // BEGIN string formats
            ModelFieldKindNativeSpec::DateTime { default: _ } => {
                let _: DateTime<Utc> = crate::imp::assert_type(name, &input.value)?;
                *field = Value::String(input.value);
                Ok(())
            }
            ModelFieldKindNativeSpec::Ip {} => {
                let _: IpAddr = crate::imp::assert_type(name, &input.value)?;
                *field = Value::String(input.value);
                Ok(())
            }
            ModelFieldKindNativeSpec::Uuid {} => {
                let _: Uuid = crate::imp::assert_type(name, &input.value)?;
                *field = Value::String(input.value);
                Ok(())
            }
            // BEGIN aggregation types
            ModelFieldKindNativeSpec::Array { .. } | ModelFieldKindNativeSpec::Object { .. } => {
                let type_ = base_field.kind.to_type();
                let value = &input.value;
                bail!("cannot set {type_} type to value {value:?}: {name:?}")
            }
        }
    }

    pub fn finalize(self) -> Result<Value> {
        todo!()
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
                value: s[m.start()..m.end() - 1].to_string(),
            })
            .ok_or_else(|| anyhow!("field name is invalid: {s} {s:?}"))
    }
}

impl InputField {
    fn cursor(&self) -> CursorIterator {
        CursorIterator {
            basename: '/'.to_string(),
            split: self.name.split('/'),
        }
    }
}

struct CursorIterator<'a> {
    basename: String,
    split: Split<'a, char>,
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
