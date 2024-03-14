#![recursion_limit = "256"]

mod ctx;

use ark_core_k8s::manager::Ctx;
use tokio::join;

pub(crate) mod consts {
    pub const NAME: &str = "vine-operator";
}

#[tokio::main]
async fn main() {
    join!(
        self::ctx::user_auth::Ctx::spawn_crd(),
        self::ctx::user_session::Ctx::spawn(),
    );
}
