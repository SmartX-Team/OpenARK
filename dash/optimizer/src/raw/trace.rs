use anyhow::Result;
use async_trait::async_trait;
use dash_optimizer_api::raw;
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

use crate::{
    ctx::OptimizerContext,
    metric::{MetricDuration, MetricSpan, MetricSpanKind},
};

#[derive(Clone)]
pub struct Reader {
    ctx: OptimizerContext,
}

#[async_trait]
impl crate::ctx::OptimizerService for Reader {
    fn new(ctx: &OptimizerContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    async fn loop_forever(self) -> Result<()> {
        info!("creating messenger: raw metrics reader");

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
                Some(MetricSpan {
                    duration: MetricDuration {
                        begin_ns: span.start_time_unix_nano,
                        end_ns: span.end_time_unix_nano,
                    },
                    kind: match span.name.as_str() {
                        "put" => MetricSpanKind::Storage {
                            type_: match get_attribute_value_str(
                                &span.attributes,
                                "code.namespace",
                            )? {
                                "dash_pipe_provider::storage::s3" => StorageType::S3,
                                _ => return None,
                            },
                        },
                        "put_metadata" => MetricSpanKind::MetadataStorage {
                            type_: match get_attribute_value_str(
                                &span.attributes,
                                "code.namespace",
                            )? {
                                "dash_pipe_provider::storage::lakehouse" => {
                                    MetadataStorageType::LakeHouse
                                }
                                _ => return None,
                            },
                        },
                        "send_one" => MetricSpanKind::Messenger {
                            topic: get_attribute_value_str(&span.attributes, "data.topic")?.into(),
                            type_: match get_attribute_value_str(
                                &span.attributes,
                                "code.namespace",
                            )? {
                                "dash_pipe_provider::messengers::kafka" => MessengerType::Kafka,
                                "dash_pipe_provider::messengers::nats" => MessengerType::Nats,
                                _ => return None,
                            },
                        },
                        _ => return None,
                    },
                    namespace: get_attribute_value_str(&span.attributes, "data.namespace")?.into(),
                    len: get_attribute_value_str(&span.attributes, "data.len")
                        .and_then(|len| len.parse().ok())?,
                })
            })
            .map(|span| self.ctx.write_metric(span))
            .collect::<FuturesUnordered<_>>()
            .try_collect::<()>()
            .await?;

        Ok(PipeMessage::with_request(&input, vec![], ()))
    }
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
