use anyhow::Result;
use async_trait::async_trait;
use dash_collector_api::{
    metadata::ObjectMetadata,
    metrics::{
        FunctionOperation, FunctionType, MessengerOperation, MetadataStorageOperation,
        MetricDuration, MetricSpan, MetricSpanKind, StorageOperation,
    },
    raw,
};
use dash_collector_world::ctx::WorldContext;
use dash_pipe_provider::{
    storage::{MetadataStorageType, StorageType},
    MessengerType, PipeArgs, PipeMessage, RemoteFunction,
};
use futures::{stream::FuturesUnordered, TryStreamExt};
use opentelemetry_proto::tonic::{
    collector::trace::v1::ExportTraceServiceRequest,
    common::v1::{any_value, KeyValue},
};
use tracing::{info, instrument, Level};

#[derive(Clone)]
pub struct Reader {
    ctx: WorldContext,
}

impl Reader {
    pub fn new(ctx: WorldContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ::dash_collector_world::service::Service for Reader {
    async fn loop_forever(self) -> Result<()> {
        info!("creating service: raw metrics reader");

        let pipe = PipeArgs::with_function(self)?
            .with_ignore_sigint(true)
            .with_model_in(Some(raw::trace::model()?))
            .with_model_out(None);
        pipe.loop_forever_async().await
    }
}

#[async_trait]
impl RemoteFunction for Reader {
    type Input = ExportTraceServiceRequest;
    type Output = ();

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call_one(
        &self,
        input: PipeMessage<<Self as RemoteFunction>::Input, ()>,
    ) -> Result<PipeMessage<<Self as RemoteFunction>::Output, ()>> {
        input
            .value
            .resource_spans
            .iter()
            // check attributes
            .filter(|spans| {
                spans
                    .resource
                    .as_ref()
                    .filter(|resource| {
                        get_attribute_value_str(&resource.attributes, "service.name")
                            .filter(|&value| value == "ark_core")
                            .is_some()
                    })
                    .is_some()
            })
            .flat_map(|spans| &spans.scope_spans)
            .flat_map(|spans| &spans.spans)
            .filter_map(|span| {
                let attributes = &span.attributes;

                Some(MetricSpan {
                    duration: MetricDuration {
                        begin_ns: span.start_time_unix_nano,
                        end_ns: span.end_time_unix_nano,
                    },
                    kind: match span.name.as_str() {
                        // Function
                        "call_function" => MetricSpanKind::Function {
                            op: FunctionOperation::Call,
                            type_: FunctionType::Dash,
                        },
                        // Messenger
                        "recv_one" => parse_messenger(attributes, MessengerOperation::Recv)?,
                        "reply_one" => parse_messenger(attributes, MessengerOperation::Reply)?,
                        "request_one" => parse_messenger(attributes, MessengerOperation::Request)?,
                        "send_one" => parse_messenger(attributes, MessengerOperation::Send)?,
                        // Metadata Storage
                        "list_metadata" => {
                            parse_metadata_storage(attributes, MetadataStorageOperation::List)?
                        }
                        "put_metadata" => {
                            parse_metadata_storage(attributes, MetadataStorageOperation::Put)?
                        }
                        // Storage
                        "get" => parse_storage(attributes, StorageOperation::Get)?,
                        "put" => parse_storage(attributes, StorageOperation::Put)?,
                        "delete" => parse_storage(attributes, StorageOperation::Delete)?,
                        // END
                        _ => return None,
                    },
                    len: get_attribute_value_str(attributes, "data.len")
                        .and_then(|len| len.parse().ok())?,
                    metadata: ObjectMetadata {
                        name: get_attribute_value_str(attributes, "data.name")?.into(),
                        namespace: get_attribute_value_str(attributes, "data.namespace")?.into(),
                    },
                })
            })
            .map(|span| self.ctx.write_metric(span))
            .collect::<FuturesUnordered<_>>()
            .try_collect::<()>()
            .await?;

        Ok(PipeMessage::with_request(&input, vec![], ()))
    }
}

fn parse_messenger(attributes: &[KeyValue], op: MessengerOperation) -> Option<MetricSpanKind> {
    Some(MetricSpanKind::Messenger {
        op,
        type_: match get_attribute_value_str(attributes, "code.namespace")? {
            "dash_pipe_provider::messengers::kafka" => MessengerType::Kafka,
            "dash_pipe_provider::messengers::nats" => MessengerType::Nats,
            _ => return None,
        },
    })
}

fn parse_metadata_storage(
    attributes: &[KeyValue],
    op: MetadataStorageOperation,
) -> Option<MetricSpanKind> {
    Some(MetricSpanKind::MetadataStorage {
        op,
        type_: match get_attribute_value_str(attributes, "code.namespace")? {
            "dash_pipe_provider::storage::lakehouse" => MetadataStorageType::LakeHouse,
            _ => return None,
        },
    })
}

fn parse_storage(attributes: &[KeyValue], op: StorageOperation) -> Option<MetricSpanKind> {
    Some(MetricSpanKind::Storage {
        op,
        type_: match get_attribute_value_str(attributes, "code.namespace")? {
            "dash_pipe_provider::storage::s3" => StorageType::S3,
            _ => return None,
        },
    })
}

fn get_attribute_value_str<'a>(attributes: &'a [KeyValue], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attr| attr.key == key)
        .and_then(|attr| attr.value.as_ref())
        .and_then(|value| value.value.as_ref())
        .and_then(|value| match value {
            any_value::Value::StringValue(value) => Some(value.as_str()),
            _ => None,
        })
}
