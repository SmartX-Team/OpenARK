mod ctx;

use ipis::tokio::{self, join};
use kiss_api::manager::Ctx;

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
