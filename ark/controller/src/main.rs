mod ctx;

use ark_core_k8s::manager::Ctx;
use tokio::join;

pub(crate) mod consts {
    pub const NAME: &str = "ark-controller";
}

#[tokio::main]
async fn main() {
    join!(
        self::ctx::job::Ctx::spawn(),
        self::ctx::package::Ctx::spawn_crd(),
    );
}
