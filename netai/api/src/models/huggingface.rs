use std::path::Path;

use ipis::{
    async_trait::async_trait,
    core::anyhow::{bail, Result},
    tokio::process::Command,
};

use crate::role::Role;

pub struct HuggingfaceModel {
    pub repo: String,
    pub role: Role,
}

#[async_trait]
impl super::Model for HuggingfaceModel {
    fn get_name(&self) -> String {
        "model.onnx".into()
    }

    fn get_namespace(&self) -> String {
        format!(
            "huggingface/{}/{}",
            &self.repo,
            self.role.to_string_kebab_case(),
        )
    }

    fn get_role(&self) -> Role {
        self.role
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
            &self.role.to_string_kebab_case(),
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
