use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use opentelemetry_proto::tonic::collector;
use tracing::{instrument, Level};

macro_rules! init_exporter {
    [ $(
            $signal:ident {
            feature : $signal_feature:expr ,
            request : $signal_request:ty ,
            response : $signal_response:ty ,
        } ,
    )* ] => {
        pub struct Exporters {
            $(
                pub $signal: Arc<dyn super::Exporter<$signal_request, $signal_response>>,
            )*
        }

        #[async_trait]
        impl super::Exporters for Exporters {
            #[instrument(level = Level::INFO, skip_all, err(Display))]
            async fn try_default() -> Result<Self> {
                use std::time::Duration;

                use ark_core_k8s::data::Name;
                use clap::Parser;
                use dash_pipe_provider::storage::lakehouse::Storage;
                use serde_json::Value;

                #[derive(Parser)]
                pub struct ExporterStorageArgs {
                    #[arg(long, env = "PIPE_FLUSH", value_name = "MS", default_value_t = 10_000)]
                    flush_ms: u64,

                    #[arg(long, env = "PIPE_MODEL_OUT", value_name = "NAME")]
                    model_out: Name,

                    #[command(flatten)]
                    s3: ::dash_pipe_api::storage::StorageS3Args,

                    #[arg(long, env = "PIPE_STORAGE_NAME", value_name = "NAME")]
                    storage_name: String,
                }

                impl ExporterStorageArgs {
                    const fn flush(&self) -> Option<Duration> {
                        ::dash_pipe_provider::storage::StorageArgs::parse_flush_ms(self.flush_ms)
                    }
                }

                let args = ExporterStorageArgs::parse();

                Ok(Self {
                    $(
                        $signal: Arc::new(
                            Storage::try_new::<Value>(
                                &args.s3,
                                args.storage_name.clone(),
                                Some(&args.model_out),
                                args.flush(),
                            ).await?
                        ),
                    )*
                })
            }

            $(
                fn $signal(&self) -> Arc<dyn super::Exporter<$signal_request, $signal_response>> {
                    self.$signal.clone()
                }
            )*

            async fn terminate(&self) -> Result<()> {
                $(
                    self.$signal.terminate().await?;
                )*
                Ok(())
            }
        }
    };
}

init_exporter![
    logs {
        feature: "logs",
        request: collector::logs::v1::ExportLogsServiceRequest,
        response: collector::logs::v1::ExportLogsServiceResponse,
    },
    metrics {
        feature: "metrics",
        request: collector::metrics::v1::ExportMetricsServiceRequest,
        response: collector::metrics::v1::ExportMetricsServiceResponse,
    },
    trace {
        feature: "trace",
        request: collector::trace::v1::ExportTraceServiceRequest,
        response: ::dash_collector_api::metrics::MetricSpan<'static>,
    },
];

#[cfg(feature = "logs")]
mod impl_logs {
    use anyhow::Result;
    use async_trait::async_trait;
    use dash_pipe_provider::{storage::lakehouse::Storage, PipeMessage};
    use opentelemetry_proto::tonic::collector;
    use tracing::{instrument, Level};

    #[async_trait]
    impl
        super::super::Exporter<
            collector::logs::v1::ExportLogsServiceRequest,
            collector::logs::v1::ExportLogsServiceResponse,
        > for Storage
    {
        #[instrument(level = Level::INFO, skip_all, err(Display))]
        async fn export(
            &self,
            _message: &PipeMessage<collector::logs::v1::ExportLogsServiceRequest, ()>,
        ) -> Result<()> {
            // TODO: to be implemented
            Ok(())
        }
    }
}

#[cfg(feature = "metrics")]
mod impl_metrics {
    use anyhow::Result;
    use async_trait::async_trait;
    use dash_pipe_provider::{storage::lakehouse::Storage, PipeMessage};
    use opentelemetry_proto::tonic::collector;
    use tracing::{instrument, Level};

    #[async_trait]
    impl
        super::super::Exporter<
            collector::metrics::v1::ExportMetricsServiceRequest,
            collector::metrics::v1::ExportMetricsServiceResponse,
        > for Storage
    {
        #[instrument(level = Level::INFO, skip_all, err(Display))]
        async fn export(
            &self,
            _message: &PipeMessage<collector::metrics::v1::ExportMetricsServiceRequest, ()>,
        ) -> Result<()> {
            // TODO: to be implemented
            Ok(())
        }
    }
}

#[cfg(feature = "trace")]
mod impl_trace {
    use std::borrow::Cow;

    use dash_collector_api::{
        metadata::ObjectMetadata,
        metrics::{
            FunctionOperation, FunctionType, MessengerOperation, MetadataStorageOperation,
            MetricDuration, MetricSpan, MetricSpanKind, StorageOperation,
        },
    };
    use dash_pipe_provider::{
        storage::{lakehouse::Storage, MetadataStorage, MetadataStorageType, StorageType},
        MessengerType, PipeMessage,
    };
    use opentelemetry_proto::tonic::common::v1::{any_value, KeyValue};

    use super::*;

    #[async_trait]
    impl
        super::super::Exporter<collector::trace::v1::ExportTraceServiceRequest, MetricSpan<'static>>
        for Storage
    {
        #[instrument(level = Level::INFO, skip_all, err(Display))]
        async fn export(
            &self,
            message: &PipeMessage<collector::trace::v1::ExportTraceServiceRequest, ()>,
        ) -> Result<()> {
            let metrics = message
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
                .filter_map(|spans| {
                    Some((&spans.resource.as_ref()?.attributes, &spans.scope_spans))
                })
                .flat_map(|(spans_attributes, scope_spans)| {
                    scope_spans
                        .iter()
                        .flat_map(|spans| &spans.spans)
                        .filter_map(move |span| {
                            let attributes = &span.attributes;

                            let code_namespace =
                                get_attribute_value_str(attributes, "code.namespace");
                            let model = get_attribute_value_str(attributes, "data.model")?.into();

                            Some(MetricSpan {
                                duration: MetricDuration {
                                    begin_ns: span.start_time_unix_nano,
                                    end_ns: span.end_time_unix_nano,
                                },
                                kind: match span.name.as_str() {
                                    // Function
                                    "call_function" => MetricSpanKind::Function {
                                        model,
                                        op: FunctionOperation::Call,
                                        type_: FunctionType::Dash,
                                    },
                                    "tick_function" => MetricSpanKind::Function {
                                        model,
                                        op: FunctionOperation::Tick,
                                        type_: FunctionType::Dash,
                                    },
                                    // Messenger
                                    "read_one" => parse_messenger(
                                        code_namespace?,
                                        model,
                                        MessengerOperation::Read,
                                    )?,
                                    "reply_one" => parse_messenger(
                                        code_namespace?,
                                        model,
                                        MessengerOperation::Reply,
                                    )?,
                                    "request_one" => parse_messenger(
                                        code_namespace?,
                                        model,
                                        MessengerOperation::Request,
                                    )?,
                                    "send_one" => parse_messenger(
                                        code_namespace?,
                                        model,
                                        MessengerOperation::Send,
                                    )?,
                                    // Metadata Storage
                                    "list_metadata" => parse_metadata_storage(
                                        code_namespace?,
                                        model,
                                        MetadataStorageOperation::List,
                                    )?,
                                    "put_metadata" => parse_metadata_storage(
                                        code_namespace?,
                                        model,
                                        MetadataStorageOperation::Put,
                                    )?,
                                    // Storage
                                    "get" => parse_storage(
                                        code_namespace?,
                                        model,
                                        StorageOperation::Get,
                                    )?,
                                    "put" => parse_storage(
                                        code_namespace?,
                                        model,
                                        StorageOperation::Put,
                                    )?,
                                    "delete" => parse_storage(
                                        code_namespace?,
                                        model,
                                        StorageOperation::Delete,
                                    )?,
                                    // END
                                    _ => return None,
                                },
                                len: get_attribute_value_str(attributes, "data.len")
                                    .and_then(|len| len.parse().ok())?,
                                metadata: ObjectMetadata {
                                    name: ["k8s.deployment.name", "k8s.job.name", "k8s.pod.name"]
                                        .into_iter()
                                        .filter_map(|key| {
                                            get_attribute_value_str(spans_attributes, key)
                                        })
                                        .next()?
                                        .into(),
                                    namespace: get_attribute_value_str(
                                        spans_attributes,
                                        "k8s.namespace.name",
                                    )?
                                    .into(),
                                },
                            })
                        })
                })
                .map(|value| PipeMessage::with_request(message, vec![], value))
                .collect::<Vec<_>>();

            // skip storing if no metrics are given
            if metrics.is_empty() {
                return Ok(());
            }

            let metrics = metrics.iter().collect::<Vec<_>>();
            self.put_metadata(&metrics).await
        }

        async fn terminate(&self) -> Result<()> {
            MetadataStorage::<MetricSpan<'static>>::flush(self).await
        }
    }

    fn parse_messenger<'a>(
        code_namespace: &str,
        model: Cow<'a, str>,
        op: MessengerOperation,
    ) -> Option<MetricSpanKind<'a>> {
        Some(MetricSpanKind::Messenger {
            model,
            op,
            type_: match code_namespace {
                "dash_pipe_provider::messengers::kafka" => MessengerType::Kafka,
                "dash_pipe_provider::messengers::nats" => MessengerType::Nats,
                _ => return None,
            },
        })
    }

    fn parse_metadata_storage<'a>(
        code_namespace: &str,
        model: Cow<'a, str>,
        op: MetadataStorageOperation,
    ) -> Option<MetricSpanKind<'a>> {
        Some(MetricSpanKind::MetadataStorage {
            model,
            op,
            type_: match code_namespace {
                "dash_pipe_provider::storage::lakehouse" => MetadataStorageType::LakeHouse,
                _ => return None,
            },
        })
    }

    fn parse_storage<'a>(
        code_namespace: &str,
        model: Cow<'a, str>,
        op: StorageOperation,
    ) -> Option<MetricSpanKind<'a>> {
        Some(MetricSpanKind::Storage {
            model,
            op,
            type_: match code_namespace {
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
}
