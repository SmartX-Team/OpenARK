use ark_api::build::{ArkBuildCrd, ArkPermissionSpec, ArkUserSpec};
use ipis::core::anyhow::{bail, Result};

pub struct Package {
    pub name: String,
    pub build: Option<ArkBuildCrd>,
    pub spec: Option<String>,
}

impl Package {
    pub(crate) const fn is_empty(&self) -> bool {
        self.build.is_none() && self.spec.is_none()
    }

    pub fn permissions(&self) -> Result<&[ArkPermissionSpec]> {
        if let Some(crd) = &self.build {
            return Ok(&crd.spec.permissions);
        }

        let name = &self.name;
        bail!("cannot get permissions of empty package: {name:?}")
    }

    pub fn user(&self) -> Result<&ArkUserSpec> {
        if let Some(crd) = &self.build {
            return Ok(&crd.spec.user);
        }

        let name = &self.name;
        bail!("cannot get permissions of empty package: {name:?}")
    }

    pub fn version(&self) -> Result<&str> {
        if let Some(crd) = &self.build {
            return Ok(crd.get_image_version());
        }

        let name = &self.name;
        bail!("cannot get version of empty package: {name:?}")
    }
}
