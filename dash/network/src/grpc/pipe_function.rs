use std::time::Duration;

use dash_network_api::{NetworkEdgeKey, NetworkNodeKey, NetworkValueBuilder};
use opentelemetry_proto::tonic::{common::v1::KeyValue, trace::v1::Span};

use super::get_attribute_value_str;

pub fn parse(
    resource_attributes: &[KeyValue],
    span: &Span,
    span_attributes: &[KeyValue],
) -> Option<(NetworkEdgeKey, NetworkValueBuilder)> {
    let kind = "function";
    let namespace = get_attribute_value_str(resource_attributes, "k8s.namespace.name")?;

    let node_from = NetworkNodeKey {
        kind: kind.into(),
        name: get_attribute_value_str(span_attributes, "data.model_from").map(Into::into),
        namespace: namespace.into(),
    };
    let node_to = NetworkNodeKey {
        kind: kind.into(),
        name: get_attribute_value_str(span_attributes, "data.model").map(Into::into),
        namespace: namespace.into(),
    };
    let key = (node_from, node_to);

    let value = NetworkValueBuilder::new(Duration::from_nanos(
        span.end_time_unix_nano - span.start_time_unix_nano,
    ));

    Some((key, value))
}
