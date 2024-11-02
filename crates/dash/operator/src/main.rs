#![recursion_limit = "256"]

mod ctx;
mod optimizer;
mod validator;

use ark_core_k8s::manager::Ctx;
use tokio::join;

pub(crate) mod consts {
    use ark_core::env::infer_string;

    pub const NAME: &str = "dash-operator";

    const ENV_PROMETHEUS_URL: &str = "PROMETHEUS_URL";

    pub fn infer_prometheus_url() -> String {
        infer_string(ENV_PROMETHEUS_URL).unwrap_or_else(|_| {
            "http://kube-prometheus-stack-prometheus.monitoring.svc:9090".into()
        })
    }
}

#[tokio::main]
async fn main() {
    join!(
        self::ctx::function::Ctx::spawn_crd(),
        self::ctx::injectors::kafka::Ctx::spawn(),
        self::ctx::injectors::nats::Ctx::spawn(),
        self::ctx::injectors::otlp::Ctx::spawn(),
        self::ctx::job::Ctx::spawn_crd(),
        self::ctx::model::Ctx::spawn_crd(),
        self::ctx::model_claim::Ctx::spawn_crd(),
        self::ctx::model_storage_binding::Ctx::spawn_crd(),
        self::ctx::storage::Ctx::spawn_crd(),
        self::ctx::task::Ctx::spawn_crd(),
    );
}
