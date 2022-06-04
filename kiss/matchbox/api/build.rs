use std::{env, io::Cursor, path::PathBuf};

use ipis::{core::anyhow::Result, tokio};

#[tokio::main]
async fn main() -> Result<()> {
    let out_dir = format!("{}/proto", env::var("OUT_DIR")?);
    let protos = [
        download(&out_dir, "matchbox/rpc/rpcpb/rpc.proto").await?,
        download(&out_dir, "matchbox/server/serverpb/messages.proto").await?,
        download(&out_dir, "matchbox/storage/storagepb/storage.proto").await?,
    ];

    for proto_path in protos {
        ::tonic_build::configure().compile(&[proto_path], &[&out_dir])?;
    }
    Ok(())
}

async fn download(out_dir: &str, path: &str) -> Result<PathBuf> {
    let repo = "https://raw.githubusercontent.com/poseidon/matchbox";
    let branch = "master";

    let url = format!("{repo}/{branch}/{path}");
    let filename: PathBuf = format!("{out_dir}/{path}").parse()?;

    // create the parent directories
    tokio::fs::create_dir_all(filename.parent().unwrap()).await?;

    let response = ::reqwest::get(url).await?;
    let mut file = tokio::fs::File::create(&filename).await?;
    let mut content = Cursor::new(response.bytes().await?);
    tokio::io::copy(&mut content, &mut file).await?;

    Ok(filename)
}
