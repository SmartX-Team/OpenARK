use kubegraph_api::vm::NetworkVirtualMachineExt;

#[tokio::main]
async fn main() {
    ::kubegraph_vm_local::NetworkVirtualMachine::main(|_| vec![]).await
}
