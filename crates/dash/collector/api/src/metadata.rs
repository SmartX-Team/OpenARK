use std::{borrow::Cow, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ObjectMetadata<'a> {
    pub name: Cow<'a, str>,
    pub namespace: Cow<'a, str>,
}

impl<'a> fmt::Display for ObjectMetadata<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { name, namespace } = self;
        write!(f, "{namespace}/{name}")
    }
}
