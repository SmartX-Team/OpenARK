mod ctx;

use ipis::tokio::{self, join};
use kiss_api::manager::Ctx;

#[tokio::main]
async fn main() {
    join!(
        self::ctx::model::Ctx::spawn_crd(),
        // self::ctx::pipe::Ctx::spawn(),
    );
}
