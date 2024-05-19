use anyhow::Result;

#[cfg(feature = "vm-local")]
pub async fn try_init() -> Result<::kubegraph_vm_local::NetworkVirtualMachine> {
    ::kubegraph_vm_local::NetworkVirtualMachine::try_default().await
}
