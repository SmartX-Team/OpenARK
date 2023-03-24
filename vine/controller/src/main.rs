mod ctx;

use ipis::tokio::{self, join};
use kiss_api::manager::Ctx;

pub(crate) mod consts {
    pub const NAME: &str = "vine-controller";
}

#[tokio::main]
async fn main() {
    join!(
        self::ctx::user_auth::Ctx::spawn_crd(),
        self::ctx::user_session::Ctx::spawn(),
    );
}
