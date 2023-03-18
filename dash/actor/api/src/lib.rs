pub mod client;
pub mod input;
mod source;

pub mod imp {
    use ipis::{
        core::anyhow::{anyhow, bail, Result},
        itertools::Itertools,
    };
    use std::{fmt, str::FromStr};

    pub fn assert_contains<'a, List, ListItem, Item>(
        name: &str,
        list_label: &str,
        list: &'a List,
        item_label: &str,
        item: Option<&Item>,
    ) -> Result<()>
    where
        &'a List: IntoIterator<Item = &'a ListItem>,
        ListItem: 'a + fmt::Debug + PartialEq<Item>,
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
}

pub mod name {
    pub const RE: &str = r"^/([a-z_-][a-z0-9_-]*[a-z0-9]?/)*$";
    pub const RE_CHILD: &str = r"^[a-z_-][a-z0-9_-]*[a-z0-9]?$";
    pub const RE_SET: &str = r"^(/[a-z_-][a-z0-9_-]*[a-z0-9]?)*/([A-Za-z0-9._-]*)$";
}

pub(crate) const NAME: &str = "dash-actor";
