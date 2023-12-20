#[cfg(feature = "logs")]
pub mod logs {
    use anyhow::Result;
    use ark_core_k8s::data::Name;

    pub fn model() -> Result<Name> {
        "dash.raw.logs".parse()
    }
}

#[cfg(feature = "metrics")]
pub mod metrics {
    use anyhow::Result;
    use ark_core_k8s::data::Name;

    pub fn model() -> Result<Name> {
        "dash.raw.metrics".parse()
    }
}

#[cfg(feature = "trace")]
pub mod trace {
    use anyhow::Result;
    use ark_core_k8s::data::Name;

    pub fn model() -> Result<Name> {
        "dash.raw.trace".parse()
    }
}
