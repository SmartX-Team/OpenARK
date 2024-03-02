use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, ops,
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Clone, Debug, Default)]
pub struct ArcNetworkGraph {
    edges: Arc<RwLock<BTreeMap<NetworkEdgeKey, NetworkValue>>>,
    nodes: Arc<RwLock<BTreeMap<NetworkNodeKey, NetworkNodeMap>>>,
}

impl ArcNetworkGraph {
    pub async fn add_edges(
        &self,
        edges: impl IntoIterator<Item = (NetworkEdgeKey, NetworkValueBuilder)>,
    ) {
        let edges = edges.into_iter();
        if edges.size_hint().0 == 0 {
            return;
        }

        let mut edges_writer = self.edges.write().await;
        let mut nodes_writer = self.nodes.write().await;

        edges.for_each(|(key, rhs)| {
            edges_writer
                .entry(key.clone())
                .and_modify(|lhs| *lhs += rhs)
                .or_insert_with(|| rhs.build());

            let (node_from, node_to) = key;
            let is_loop = node_from == node_to;
            {
                let node = nodes_writer
                    .entry(node_from.clone())
                    .or_insert_with(NetworkNodeMap::default);
                if is_loop {
                    node.r#loop = true;
                } else {
                    node.to.insert(node_to.clone());
                }
            }
            {
                let node = nodes_writer
                    .entry(node_to)
                    .or_insert_with(NetworkNodeMap::default);
                if is_loop {
                    node.r#loop = true;
                } else {
                    node.from.insert(node_from);
                }
            }
        })
    }

    pub async fn get_edge(&self, key: &NetworkEdgeKey) -> Option<NetworkValue> {
        self.edges.read().await.get(key).cloned()
    }

    pub async fn get_node(&self, key: &NetworkNodeKey) -> Option<NetworkNode> {
        let NetworkNodeMap { from, r#loop, to } = {
            let nodes_reader = self.nodes.read().await;
            nodes_reader.get(key).cloned()?
        };

        let edges_reader = self.edges.read().await;
        Some(NetworkNode {
            from: from
                .into_iter()
                .filter_map(|from| {
                    Some((
                        from.clone(),
                        edges_reader.get(&(from, key.clone()))?.clone(),
                    ))
                })
                .collect(),
            r#loop: if r#loop {
                edges_reader.get(&(key.clone(), key.clone())).cloned()
            } else {
                None
            },
            to: to
                .into_iter()
                .filter_map(|to| Some((to.clone(), edges_reader.get(&(key.clone(), to))?.clone())))
                .collect(),
        })
    }

    pub async fn to_json(&self) -> NetworkGraph<String, String> {
        let edges_reader = self.edges.read().await;
        let nodes_reader = self.nodes.read().await;

        NetworkGraph {
            edges: edges_reader.iter().fold(
                BTreeMap::<_, BTreeMap<_, _>>::default(),
                |mut writer, ((from, to), value)| {
                    writer
                        .entry(from.to_string())
                        .or_default()
                        .entry(to.to_string())
                        .or_insert(value.to_json());
                    writer
                },
            ),
            nodes: nodes_reader
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_json()))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkGraph<Key = NetworkEdgeKey, Bucket = Duration>
where
    Bucket: Ord,
    Key: Ord,
{
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub edges: BTreeMap<Key, BTreeMap<Key, NetworkValue<Bucket>>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nodes: BTreeMap<Key, NetworkNodeMap<Key>>,
}

pub type NetworkEdgeKey = (NetworkNodeKey, NetworkNodeKey);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkNode<Key = NetworkNodeKey, Bucket = Duration>
where
    Bucket: Ord,
    Key: Ord,
{
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub from: BTreeMap<Key, NetworkValue<Bucket>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#loop: Option<NetworkValue<Bucket>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub to: BTreeMap<Key, NetworkValue<Bucket>>,
}

impl<Key> NetworkNode<Key>
where
    Key: Ord,
{
    pub fn into_json(self) -> NetworkNode<String, String>
    where
        Key: ToString,
    {
        let Self { from, r#loop, to } = self;
        NetworkNode {
            from: from
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.into_json()))
                .collect(),
            r#loop: r#loop.map(|value| value.into_json()),
            to: to
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.into_json()))
                .collect(),
        }
    }

    pub fn to_json(&self) -> NetworkNode<String, String>
    where
        Key: ToString,
    {
        let Self { from, r#loop, to } = self;
        NetworkNode {
            from: from
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_json()))
                .collect(),
            r#loop: r#loop.as_ref().map(|value| value.to_json()),
            to: to
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_json()))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkNodeMap<Key = NetworkNodeKey>
where
    Key: Ord,
{
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub from: BTreeSet<Key>,
    #[serde(default, skip_serializing_if = "is_bool_default")]
    pub r#loop: bool,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub to: BTreeSet<Key>,
}

impl<Key> Default for NetworkNodeMap<Key>
where
    Key: Ord,
{
    fn default() -> Self {
        Self {
            from: BTreeSet::default(),
            r#loop: false,
            to: BTreeSet::default(),
        }
    }
}

impl<Key> NetworkNodeMap<Key>
where
    Key: Ord,
{
    fn to_json(&self) -> NetworkNodeMap<String>
    where
        Key: ToString,
    {
        let Self { from, r#loop, to } = self;
        NetworkNodeMap {
            from: from.iter().map(|key| key.to_string()).collect(),
            r#loop: *r#loop,
            to: to.iter().map(|key| key.to_string()).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NetworkNodeKey {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub namespace: String,
}

impl fmt::Display for NetworkNodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            kind,
            name,
            namespace,
        } = self;

        let name = name.as_ref().map(|name| name.as_str()).unwrap_or("_");

        write!(f, "{kind}/{namespace}/{name}")
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkValue<Bucket = Duration>
where
    Bucket: Ord,
{
    #[serde(default)]
    pub count: usize,
    #[serde(default)]
    pub duration: NetworkHistogram<Bucket, usize>,
}

impl ops::AddAssign for NetworkValue {
    fn add_assign(&mut self, rhs: Self) {
        self.count += rhs.count;
        self.duration += rhs.duration;
    }
}

impl ops::AddAssign<NetworkValueBuilder> for NetworkValue {
    fn add_assign(&mut self, rhs: NetworkValueBuilder) {
        self.count += 1;
        self.duration += rhs;
    }
}

impl NetworkValue {
    pub fn into_json(self) -> NetworkValue<String> {
        let Self { count, duration } = self;
        NetworkValue {
            count,
            duration: duration.into_json(),
        }
    }

    pub fn to_json(&self) -> NetworkValue<String> {
        let Self { count, duration } = self;
        NetworkValue {
            count: *count,
            duration: duration.to_json(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct NetworkValueBuilder {
    duration: Duration,
}

impl NetworkValueBuilder {
    const DEFAULT_BUCKETS: &'static [Duration] = &[
        Duration::from_millis(1 << 1),
        Duration::from_millis(1 << 2),
        Duration::from_millis(1 << 3),
        Duration::from_millis(1 << 4),
        Duration::from_millis(1 << 5),
        Duration::from_millis(1 << 6),
        Duration::from_millis(1 << 7),
        Duration::from_millis(1 << 8),
        Duration::from_millis(1 << 9),
        Duration::from_millis(1 << 10),
        Duration::from_millis(1 << 11),
        Duration::from_millis(1 << 12),
        Duration::from_millis(1 << 13),
        Duration::from_millis(1 << 14),
        Duration::from_millis(1 << 15),
        Duration::from_millis(1 << 16),
    ];

    pub fn new(duration: Duration) -> Self {
        Self { duration }
    }

    fn build(self) -> NetworkValue {
        let Self { duration } = self;

        NetworkValue {
            count: 1,
            duration: NetworkHistogram(
                Self::DEFAULT_BUCKETS
                    .iter()
                    .copied()
                    .map(|le| (le, (duration < le) as usize))
                    .collect(),
            ),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, transparent)]
pub struct NetworkHistogram<Bucket, Value>(pub BTreeMap<Bucket, Value>)
where
    Bucket: Ord;

impl<Bucket, Value> Default for NetworkHistogram<Bucket, Value>
where
    Bucket: Default + Ord,
{
    fn default() -> Self {
        Self(BTreeMap::default())
    }
}

impl<Value> ops::AddAssign for NetworkHistogram<Duration, Value>
where
    Value: Copy + ops::AddAssign,
{
    fn add_assign(&mut self, rhs: Self) {
        for (duration, rhs) in rhs.0 {
            self.0
                .entry(duration)
                .and_modify(|lhs| *lhs += rhs)
                .or_insert(rhs);
        }
    }
}

impl ops::AddAssign<NetworkValueBuilder> for NetworkHistogram<Duration, usize> {
    fn add_assign(&mut self, rhs: NetworkValueBuilder) {
        let NetworkValueBuilder { duration } = rhs;

        self.0
            .iter_mut()
            .filter(|(&le, _)| duration < le)
            .for_each(|(_, lhs)| *lhs += 1);
    }
}

impl<Value> NetworkHistogram<Duration, Value>
where
    Value: Clone,
{
    fn into_json(self) -> NetworkHistogram<String, Value> {
        let Self(buckets) = self;
        NetworkHistogram(
            buckets
                .into_iter()
                .map(|(key, value)| (key.as_millis().to_string(), value))
                .collect(),
        )
    }

    fn to_json(&self) -> NetworkHistogram<String, Value> {
        let Self(buckets) = self;
        NetworkHistogram(
            buckets
                .iter()
                .map(|(key, value)| (key.as_millis().to_string(), value.clone()))
                .collect(),
        )
    }
}

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

const fn is_bool_default(value: &bool) -> bool {
    !*value
}
