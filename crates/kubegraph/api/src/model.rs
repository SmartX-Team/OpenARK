use anyhow::Result;
use ark_core_k8s::data::Name;

pub fn connector() -> Result<Name> {
    "dash.network.connector".parse()
}

pub fn data() -> Result<Name> {
    "dash.network.data".parse()
}
