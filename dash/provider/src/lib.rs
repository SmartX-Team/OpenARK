pub mod client;
pub mod input;
pub mod storage;

pub mod imp {
    use anyhow::{anyhow, bail, Result};
    use dash_api::model::ModelFieldKindStringSpec;
    use itertools::Itertools;
    use std::{fmt, str::FromStr};

    pub fn assert_contains<'a, List, Item>(
        name: &str,
        list_label: &str,
        list: &'a List,
        item_label: &str,
        item: Option<Item>,
    ) -> Result<()>
    where
        &'a List: IntoIterator,
        <&'a List as IntoIterator>::Item: 'a + fmt::Debug + PartialEq<Item>,
        Item: fmt::Debug,
    {
        match item {
            Some(item) => {
                if list.into_iter().any(|list_item| list_item == item) {
                    Ok(())
                } else {
                    let items = list
                        .into_iter()
                        .map(|list_item| format!("{list_item:?}"))
                        .join(", ");
                    bail!(
                        "{item_label} value {item:?} should be one of {list_label} ({items}): {name:?}",
                    )
                }
            }
            _ => Ok(()),
        }
    }

    pub fn assert_string(name: &str, value: &str, spec: &ModelFieldKindStringSpec) -> Result<()> {
        let value_len: u32 = match value.len().try_into() {
            Ok(len) => len,
            Err(_) => bail!("too long string value: {name:?}"),
        };

        match spec {
            ModelFieldKindStringSpec::Dynamic {} => Ok(()),
            ModelFieldKindStringSpec::Static { length } => {
                if value_len == *length {
                    Ok(())
                } else {
                    bail!("string value {value:?} should be fixed length: expected length {length:?}, but given {value_len:?}: {name:?}")
                }
            }
            ModelFieldKindStringSpec::Range { minimum, maximum } => {
                if let Some(minimum) = minimum {
                    if value_len < *minimum {
                        bail!("string value {value:?} should be more longer: expected at least {minimum:?}, but given {value_len:?}: {name:?}");
                    }
                }

                if value_len > *maximum {
                    bail!("string value {value:?} should be more shorter: expected at most {maximum:?}, but given {value_len:?}: {name:?}");
                }
                Ok(())
            }
        }
    }

    pub fn assert_type<Type, Item>(name: &str, item: Item) -> Result<Type>
    where
        Type: FromStr,
        <Type as FromStr>::Err: fmt::Display,
        Item: AsRef<str>,
    {
        let item = item.as_ref();
        item.parse::<Type>()
            .map_err(|e| anyhow!("cannot parse value {item:?}: {name:?}: {e}"))
    }

    pub fn get_children_names<I>(names: I) -> Vec<String>
    where
        I: IntoIterator,
        <I as IntoIterator>::Item: AsRef<str>,
    {
        names
            .into_iter()
            .filter_map(|name| {
                name.as_ref()
                    .split('/')
                    .rev()
                    .nth(1)
                    .map(ToString::to_string)
            })
            .collect()
    }

    pub fn parse_api_version(api_version: &str) -> Result<(&str, &str)> {
        let mut attrs: Vec<_> = api_version.split('/').collect();
        if attrs.len() != 2 {
            let crd_name = api_version;
            bail!("CRD name is invalid; expected name/version, but given {crd_name} {crd_name:?}",);
        }

        let version = attrs.pop().unwrap();
        let crd_name = attrs.pop().unwrap();
        Ok((crd_name, version))
    }
}

pub mod name {
    pub const RE: &str = r"^/([a-z_-][a-z0-9_-]*[a-z0-9]?/)*$";
    pub const RE_CHILD: &str = r"^[a-z_-][a-z0-9_-]*[a-z0-9]?$";
    pub const RE_SET: &str = r"^(/[1-9]?[0-9]+|/[a-z_-][a-z0-9_-]*[a-z0-9]?)*/([A-Za-z0-9._-]*)$";
}

pub(crate) const NAME: &str = "dash-provider";
