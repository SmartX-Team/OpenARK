mod ctx;

use ipis::tokio;
use kiss_api::manager::Ctx;

pub(crate) mod consts {
    pub const NAME: &str = "ark-controller";
}

#[tokio::main]
async fn main() {
    self::ctx::Ctx::spawn_crd().await
}
