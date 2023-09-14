use anyhow::Result;

#[cfg(unix)]
pub async fn get_cluster_domain() -> Result<String> {
    use tokio::{fs::File, io::AsyncReadExt};

    // Read the file
    let mut buf = Default::default();
    let mut f = File::open("/etc/resolv.conf").await?;
    f.read_to_end(&mut buf).await?;

    // Parse the buffer
    let cfg = ::resolv_conf::Config::parse(&buf)?;
    Ok(cfg
        .get_search()
        .and_then(|list| list.iter().min_by_key(|search| search.len()))
        .map(AsRef::as_ref)
        .unwrap_or("ops.openark")
        .into())
}
