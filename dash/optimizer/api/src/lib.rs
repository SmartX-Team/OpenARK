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
    use dash_collector_api::metadata::ObjectMetadata;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    pub fn model_in() -> Result<Name> {
        "dash.optimize.storage.in".parse()
    }

    pub fn model_out() -> Result<Name> {
        "dash.optimize.storage.out".parse()
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
    pub struct Request<'a> {
        #[serde(default)]
        pub policy: ModelClaimBindingPolicy,
        #[serde(flatten)]
        pub storage: ObjectMetadata<'a>,
    }

    pub type Response = Option<String>;
}
