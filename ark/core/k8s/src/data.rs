use std::{borrow::Borrow, cmp::Ordering, fmt, ops, str::FromStr};

use anyhow::{bail, Error};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use strum::{Display, EnumString};

#[derive(
    Copy, Clone, Debug, Display, EnumString, PartialEq, Serialize, Deserialize, JsonSchema,
)]
pub enum ImagePullPolicy {
    Always,
    IfNotPresent,
    Never,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Name(String);

impl FromStr for Name {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self, <Self as FromStr>::Err> {
        let re = Regex::new(r"^[a-z][a-z0-9_-]*[a-z0-9]?$")?;
        if re.is_match(name) {
            Ok(Self(name.into()))
        } else {
            bail!("invalid name: {name:?}")
        }
    }
}

impl From<Name> for String {
    fn from(value: Name) -> Self {
        value.0
    }
}

// #[cfg(feature = "nats")]
// impl From<Name> for ::async_nats::Subject {
//     fn from(value: Name) -> Self {
//         value.0.into()
//     }
// }

impl Borrow<str> for Name {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Borrow<String> for Name {
    fn borrow(&self) -> &String {
        &self.0
    }
}

impl ops::Deref for Name {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <String as fmt::Debug>::fmt(&self.0, f)
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <String as fmt::Display>::fmt(&self.0, f)
    }
}

impl PartialEq<String> for Name {
    fn eq(&self, other: &String) -> bool {
        self.0.eq(other)
    }
}

impl PartialEq<Name> for String {
    fn eq(&self, other: &Name) -> bool {
        self.eq(&other.0)
    }
}

impl PartialOrd<String> for Name {
    fn partial_cmp(&self, other: &String) -> Option<Ordering> {
        self.0.partial_cmp(other)
    }

    fn lt(&self, other: &String) -> bool {
        self.0.lt(other)
    }

    fn le(&self, other: &String) -> bool {
        self.0.le(other)
    }

    fn gt(&self, other: &String) -> bool {
        self.0.gt(other)
    }

    fn ge(&self, other: &String) -> bool {
        self.0.ge(other)
    }
}

impl PartialOrd<Name> for String {
    fn partial_cmp(&self, other: &Name) -> Option<Ordering> {
        self.partial_cmp(&other.0)
    }

    fn lt(&self, other: &Name) -> bool {
        self.lt(&other.0)
    }

    fn le(&self, other: &Name) -> bool {
        self.le(&other.0)
    }

    fn gt(&self, other: &Name) -> bool {
        self.gt(&other.0)
    }

    fn ge(&self, other: &Name) -> bool {
        self.ge(&other.0)
    }
}

impl<'de> Deserialize<'de> for Name {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        <String as Deserialize<'de>>::deserialize(deserializer)
            .and_then(|name| Self::from_str(&name).map_err(::serde::de::Error::custom))
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Url(pub ::url::Url);

impl FromStr for Url {
    type Err = <::url::Url as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        <::url::Url as FromStr>::from_str(s).map(Self)
    }
}

impl ops::Deref for Url {
    type Target = ::url::Url;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Debug for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <::url::Url as fmt::Debug>::fmt(&self.0, f)
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <::url::Url as fmt::Display>::fmt(&self.0, f)
    }
}

impl PartialOrd for Url {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(<Self as Ord>::cmp(self, other))
    }
}

impl Ord for Url {
    fn cmp(&self, other: &Self) -> Ordering {
        <str as Ord>::cmp(self.0.as_str(), other.0.as_str())
    }
}

impl JsonSchema for Url {
    fn is_referenceable() -> bool {
        false
    }

    fn schema_name() -> String {
        "Url".into()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        String::json_schema(gen)
    }
}
