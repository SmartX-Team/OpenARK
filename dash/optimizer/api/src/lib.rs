pub mod optimize {
    use dash_api::{
        model::ModelCrd, model_claim::ModelClaimBindingPolicy,
        model_storage_binding::ModelStorageBindingStorageKind, storage::ModelStorageKind,
    };
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
    pub struct Request {
        pub model: Option<ModelCrd>,
        pub policy: Option<ModelClaimBindingPolicy>,
        pub storage: Option<ModelStorageKind>,
    }

    pub type Response = Option<ModelStorageBindingStorageKind<String>>;
}

pub mod topics {
    use anyhow::Result;
    use ark_core_k8s::data::Name;

    pub fn optimize_model_in() -> Result<Name> {
        "dash.optimize.model.in".parse()
    }

    pub fn optimize_model_out() -> Result<Name> {
        "dash.optimize.model.out".parse()
    }

    pub fn raw_logs() -> Result<Name> {
        "dash.raw.logs".parse()
    }

    pub fn raw_metrics() -> Result<Name> {
        "dash.raw.metrics".parse()
    }

    pub fn raw_trace() -> Result<Name> {
        "dash.raw.trace".parse()
    }
}
