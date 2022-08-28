#![deny(
    clippy::all,
    clippy::cargo,
    clippy::nursery,
    clippy::pedantic,
    clippy::restriction
)]

mod ctx;

use ipis::tokio;
use kiss_api::manager::Ctx;

#[tokio::main]
async fn main() {
    self::ctx::Ctx::spawn_crd().await
}
