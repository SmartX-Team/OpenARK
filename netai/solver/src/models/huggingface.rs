use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::{StreamExt, TryFutureExt};
use reqwest::Response;
use serde::de::DeserializeOwned;
use tokio::{fs, io, process::Command};

use crate::role::Role;

pub struct Model {
    pub repo: String,
    pub role: Role,
}

#[async_trait]
impl super::Model for Model {
    fn get_name(&self) -> String {
        "model.onnx".into()
    }

    fn get_namespace(&self) -> String {
        format!(
            "huggingface/{}/{}",
            &self.repo,
            self.role.as_huggingface_feature(),
        )
    }

    fn get_repo(&self) -> String {
        self.repo.clone()
    }

    fn get_role(&self) -> Role {
        self.role
    }

    fn get_kind(&self) -> super::ModelKind {
        super::ModelKind::Huggingface
    }

    async fn get_readme(&self) -> Result<Option<String>> {
        try_get_text(&self.repo, "README.md").await
    }

    async fn get_license(&self) -> Result<Option<String>> {
        const BLOCK: &str = "---";
        const KEY: &str = "license:";

        match try_get_text(&self.repo, "README.md").await? {
            Some(text) if text.starts_with(BLOCK) => {
                match text
                    .lines()
                    .skip(1)
                    .enumerate()
                    .find(|(_, line)| line.starts_with(BLOCK))
                    .map(|(index, _)| index)
                {
                    Some(num_lines) => Ok(text
                        .lines()
                        .skip(1)
                        .take(num_lines)
                        .find(|line| line.starts_with(KEY))
                        .map(|line| line[KEY.len()..].trim().to_string())),
                    None => Ok(None),
                }
            }
            Some(_) | None => Ok(None),
        }
    }

    async fn download_to(&self, path: &Path) -> Result<()> {
        const CONVERTER_MODULE_NAME: &str = "transformers.onnx";

        let mut command = Command::new("python3");
        command.args([
            "-m",
            CONVERTER_MODULE_NAME,
            "--model",
            &self.repo,
            "--feature",
            self.role.as_huggingface_feature(),
            &path
                .parent()
                .expect("namespace path should be exists")
                .display()
                .to_string(),
        ]);

        let output = command.spawn()?.wait_with_output().await?;
        if !output.status.success() {
            bail!("failed to execute the command: {:?}", command.as_std());
        }
        Ok(())
    }

    async fn verify(&self, path: &Path) -> Result<bool> {
        self.download_to(path).await.map(|()| true)
    }
}

pub async fn get_file(repo: &str, name: &str) -> Result<PathBuf> {
    let (_, response) = try_request_raw(repo, name).await?;

    let mut byte_stream = response.bytes_stream();

    let path: PathBuf = format!("/models/{name}").parse()?;
    let mut file = fs::File::create(&path).await?;

    while let Some(item) = byte_stream.next().await {
        io::copy(&mut item?.as_ref(), &mut file).await?;
    }
    Ok(path)
}

async fn try_get_text(repo: &str, name: &str) -> Result<Option<String>> {
    try_request(repo, name)
        .and_then(|response| async {
            match response {
                Some(response) => response.text().await.map(Some).map_err(Into::into),
                None => Ok(None),
            }
        })
        .await
}

pub async fn get_json<T>(repo: &str, name: &str) -> Result<T>
where
    T: Default + DeserializeOwned,
{
    request(repo, name)
        .and_then(|response| response.json().map_err(Into::into))
        .await
}

async fn request(repo: &str, name: &str) -> Result<Response> {
    let (url, response) = try_request_raw(repo, name).await?;

    if response.status().is_success() {
        Ok(response)
    } else {
        let status = response.status();
        bail!("failed to download file: [{status}] {url}");
    }
}

async fn try_request(repo: &str, name: &str) -> Result<Option<Response>> {
    let (_, response) = try_request_raw(repo, name).await?;

    if response.status().is_success() {
        Ok(Some(response))
    } else {
        Ok(None)
    }
}

async fn try_request_raw(repo: &str, name: &str) -> Result<(String, Response)> {
    let url = format!("https://huggingface.co/{repo}/raw/main/{name}");
    reqwest::get(&url)
        .await
        .map(|response| (url, response))
        .map_err(Into::into)
}
