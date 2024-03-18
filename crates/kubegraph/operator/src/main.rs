mod ctx;

use ark_core_k8s::manager::Ctx;

pub(crate) mod consts {
    pub const NAME: &str = "kubegraph-operator";
}

#[tokio::main]
async fn main() {
    self::ctx::connector::Ctx::spawn_crd().await
}
