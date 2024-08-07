use kube::{CustomResource, CustomResourceExt};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    graph::{GraphFilter, GraphMetadataPinned, GraphScope},
    resource::NetworkResource,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(bound = "
    M: Default + JsonSchema,
")]
#[serde(
    rename_all = "camelCase",
    bound = "
        M: Default + Serialize + DeserializeOwned,
    "
)]
pub struct VirtualProblem<M = GraphMetadataPinned> {
    pub filter: GraphFilter,
    #[serde(flatten)]
    pub scope: GraphScope,
    #[serde(default)]
    pub spec: ProblemSpec<M>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    CustomResource,
)]
#[kube(
    group = "kubegraph.ulagbulag.io",
    version = "v1alpha1",
    kind = "NetworkProblem",
    root = "NetworkProblemCrd",
    shortname = "np",
    namespaced,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "problem version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[schemars(bound = "M: Default + JsonSchema")]
#[serde(
    rename_all = "camelCase",
    bound = "M: Default + Serialize + DeserializeOwned"
)]
pub struct ProblemSpec<M = GraphMetadataPinned> {
    #[serde(default)]
    pub metadata: M,

    #[serde(default = "ProblemSpec::<M>::default_verbose")]
    pub verbose: bool,
}

impl<M> Default for ProblemSpec<M>
where
    M: Default,
{
    fn default() -> Self {
        Self {
            metadata: M::default(),
            verbose: Self::default_verbose(),
        }
    }
}

impl NetworkResource for NetworkProblemCrd {
    type Filter = ();

    fn description(&self) -> String {
        <Self as NetworkResource>::type_name().into()
    }

    fn type_name() -> &'static str
    where
        Self: Sized,
    {
        <Self as CustomResourceExt>::crd_name()
    }
}

impl<M> ProblemSpec<M> {
    pub const MAX_CAPACITY: u64 = u64::MAX >> 32;

    const fn default_verbose() -> bool {
        false
    }
}
