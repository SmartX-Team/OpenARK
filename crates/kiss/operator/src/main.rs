mod ctx;

use ark_core_k8s::manager::Ctx;

pub(crate) mod consts {
    pub const NAME: &str = "kiss-operator";
}

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    self::ctx::Ctx::spawn_crd().await
}
