use anyhow::Result;
use ark_core_k8s::data::Name;

pub fn connector() -> Result<Name> {
    "dash.network.connector".parse()
}
