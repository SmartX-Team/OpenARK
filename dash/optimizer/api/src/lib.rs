use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ObjectMetadata {
    pub name: String,
    pub namespace: String,
}

impl fmt::Display for ObjectMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { name, namespace } = self;
        write!(f, "{namespace}/{name}")
    }
}

pub mod optimize {
    pub mod model {
        use anyhow::Result;
        use ark_core_k8s::data::Name;
        use dash_api::{
            model::ModelCrd, model_claim::ModelClaimBindingPolicy,
            model_storage_binding::ModelStorageBindingStorageKind, storage::ModelStorageKind,
        };
        use schemars::JsonSchema;
        use serde::{Deserialize, Serialize};

        pub fn model_in() -> Result<Name> {
            "dash.optimize.model.in".parse()
        }

        pub fn model_out() -> Result<Name> {
            "dash.optimize.model.out".parse()
        }

        #[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
        pub struct Request {
            #[serde(default)]
            pub model: Option<ModelCrd>,
            #[serde(default)]
            pub policy: ModelClaimBindingPolicy,
            #[serde(default)]
            pub storage: Option<ModelStorageKind>,
        }

        pub type Response = Option<ModelStorageBindingStorageKind<String>>;
    }

    pub mod storage {
        use anyhow::Result;
        use ark_core_k8s::data::Name;
        use dash_api::model_claim::ModelClaimBindingPolicy;
        use schemars::JsonSchema;
        use serde::{Deserialize, Serialize};

        pub fn model_in() -> Result<Name> {
            "dash.optimize.storage.in".parse()
        }

        pub fn model_out() -> Result<Name> {
            "dash.optimize.storage.out".parse()
        }

        #[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
        pub struct Request {
            #[serde(default)]
            pub policy: ModelClaimBindingPolicy,
            #[serde(flatten)]
            pub storage: crate::ObjectMetadata,
        }

        pub type Response = Option<String>;
    }
}

pub mod raw {
    pub mod logs {
        use anyhow::Result;
        use ark_core_k8s::data::Name;

        pub fn model() -> Result<Name> {
            "dash.raw.logs".parse()
        }
    }

    pub mod metrics {
        use anyhow::Result;
        use ark_core_k8s::data::Name;

        pub fn model() -> Result<Name> {
            "dash.raw.metrics".parse()
        }
    }

    pub mod trace {
        use anyhow::Result;
        use ark_core_k8s::data::Name;

        pub fn model() -> Result<Name> {
            "dash.raw.trace".parse()
        }
    }
}
