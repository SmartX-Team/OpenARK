mod actix;
mod routes;
mod vm;

use kubegraph_api::vm::NetworkVirtualMachineExt;
use tokio::spawn;

#[tokio::main]
async fn main() {
    self::vm::NetworkVirtualMachine::main(|(signal, vm)| {
        vec![spawn(crate::actix::loop_forever(
            signal.clone(),
            vm.clone(),
        ))]
    })
    .await
}
