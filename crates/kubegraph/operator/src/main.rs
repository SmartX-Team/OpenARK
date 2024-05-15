mod ctx;

use ark_core_k8s::manager::Ctx;
use tokio::join;

pub(crate) mod consts {
    pub const NAME: &str = "kubegraph-operator";
}

#[tokio::main]
async fn main() {
    join!(
        self::ctx::connector::Ctx::spawn_crd(),
        self::ctx::function::Ctx::spawn_crd(),
        self::ctx::problem::Ctx::spawn_crd(),
    );
}
