mod ctx;

use ark_core_k8s::manager::Ctx;

pub(crate) mod consts {
    pub const NAME: &str = "kiss-controller";
}

#[tokio::main]
async fn main() {
    self::ctx::Ctx::spawn_crd().await
}
