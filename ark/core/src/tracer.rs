use std::{env, ffi::OsStr};

#[cfg(feature = "otlp")]
use opentelemetry_otlp as otlp;
use tracing::{dispatcher, Subscriber};
use tracing_subscriber::{
    layer::SubscriberExt, registry::LookupSpan, util::SubscriberInitExt, Layer, Registry,
};

fn init_once_opentelemetry(export: bool) {
    #[cfg(feature = "otlp")]
    use opentelemetry_sdk::runtime::Tokio as Runtime;

    // Skip init if has been set
    if dispatcher::has_been_set() {
        return;
    }

    // Set default service name
    {
        const SERVICE_NAME_KEY: &str = "OTEL_SERVICE_NAME";
        const SERVICE_NAME_VALUE: &str = env!("CARGO_CRATE_NAME");

        if env::var_os(SERVICE_NAME_KEY).is_none() {
            env::set_var(SERVICE_NAME_KEY, SERVICE_NAME_VALUE);
        }
    }

    #[cfg(feature = "otlp")]
    fn init_otlp_pipeline() -> otlp::TonicExporterBuilder {
        otlp::new_exporter().tonic()
    }

    fn init_layer_env_filter<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        ::tracing_subscriber::EnvFilter::from_default_env()
    }

    fn init_layer_stdfmt<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        ::tracing_subscriber::fmt::layer()
    }

    #[cfg(feature = "logs")]
    fn init_layer_otlp_logger<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        otlp::new_pipeline()
            .logging()
            .with_exporter(init_otlp_pipeline())
            .install_batch(Runtime)
            .map(|_| todo!())
            .expect("failed to init a logger")
    }

    #[cfg(feature = "metrics")]
    fn init_layer_otlp_metrics<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        otlp::new_pipeline()
            .metrics(Runtime)
            .with_exporter(init_otlp_pipeline())
            .build()
            .map(::tracing_opentelemetry::MetricsLayer::new)
            .expect("failed to init a metrics")
    }

    #[cfg(feature = "trace")]
    fn init_layer_otlp_tracer<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        otlp::new_pipeline()
            .tracing()
            .with_exporter(init_otlp_pipeline())
            .install_batch(Runtime)
            .map(::tracing_opentelemetry::OpenTelemetryLayer::new)
            .expect("failed to init a tracer")
    }

    let layer = Registry::default()
        .with(init_layer_env_filter())
        .with(init_layer_stdfmt());

    if export {
        #[cfg(feature = "logs")]
        let layer = layer.with(init_layer_otlp_logger());
        #[cfg(feature = "metrics")]
        let layer = layer.with(init_layer_otlp_metrics());
        #[cfg(feature = "trace")]
        let layer = layer.with(init_layer_otlp_tracer());

        layer.init()
    } else {
        layer.init()
    }
}

pub fn init_once() {
    init_once_with_default(true)
}

pub fn init_once_with(level: impl AsRef<OsStr>, export: bool) {
    // Skip init if has been set
    if dispatcher::has_been_set() {
        return;
    }

    // set custom tracing level
    ::std::env::set_var(KEY, level);

    init_once_opentelemetry(export)
}

pub fn init_once_with_default(export: bool) {
    // Skip init if has been set
    if dispatcher::has_been_set() {
        return;
    }

    // set default tracing level
    if ::std::env::var_os(KEY).is_none() {
        ::std::env::set_var(KEY, "INFO");
    }

    init_once_opentelemetry(export)
}

pub fn init_once_with_level_int(level: u8, export: bool) {
    // You can see how many times a particular flag or argument occurred
    // Note, only flags can have multiple occurrences
    let debug_level = match level {
        0 => "WARN",
        1 => "INFO",
        2 => "DEBUG",
        3 => "TRACE",
        level => panic!("too high debug level: {level}"),
    };
    env::set_var("RUST_LOG", debug_level);
    init_once_with(debug_level, export)
}

const KEY: &str = "RUST_LOG";
