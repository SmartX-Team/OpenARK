use ark_actor_api::{
    args::{ActorArgs, PackageFlags},
    repo::RepositoryManager,
};
use ark_api::package::ArkPackageCrd;
use ipis::{async_trait, core::anyhow::Result};
use kube::{Api, Client};

pub struct PackageManager {
    flags: PackageFlags,
    repos: RepositoryManager,
}

impl PackageManager {
    pub async fn try_new(args: &ActorArgs) -> Result<Self> {
        Ok(Self {
            flags: args.flags.clone(),
            repos: RepositoryManager::try_from_local(&args.repository_home).await?,
        })
    }

    pub fn create_session<'kube>(&self, kube: &'kube Client) -> PackageSession<'kube, '_> {
        PackageSession {
            kube,
            manager: self,
        }
    }

    pub async fn try_into_owned_session(self) -> Result<PackageSessionOwned> {
        Ok(PackageSessionOwned {
            kube: Client::try_default().await?,
            manager: self,
        })
    }
}

pub struct PackageSessionOwned {
    kube: Client,
    manager: PackageManager,
}

#[async_trait::async_trait]
impl ::ark_actor_api::PackageManager for PackageSessionOwned {
    async fn exists(&self, name: &str) -> Result<bool> {
        let Self { kube, manager } = self;
        let session = PackageSession { kube, manager };
        session.exists(name).await
    }

    async fn add(&self, name: &str) -> Result<()> {
        let Self { kube, manager } = self;
        let session = PackageSession { kube, manager };
        session.add(name).await
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let Self { kube, manager } = self;
        let session = PackageSession { kube, manager };
        session.delete(name).await
    }

    async fn run(&self, name: &str, args: &[String]) -> Result<()> {
        let Self { kube, manager } = self;
        let session = PackageSession { kube, manager };
        session.run(name, args).await
    }
}

pub struct PackageSession<'kube, 'manager> {
    kube: &'kube Client,
    manager: &'manager PackageManager,
}

#[async_trait::async_trait]
impl<'kube, 'manager> ::ark_actor_api::PackageManager for PackageSession<'kube, 'manager> {
    async fn exists(&self, name: &str) -> Result<bool> {
        let api = Api::<ArkPackageCrd>::default_namespaced(self.kube.clone());
        api.get_opt(name)
            .await
            .map(|object| object.is_some())
            .map_err(Into::into)
    }

    async fn add(&self, name: &str) -> Result<()> {
        todo!()
    }

    async fn delete(&self, name: &str) -> Result<()> {
        todo!()
    }

    async fn run(&self, name: &str, args: &[String]) -> Result<()> {
        todo!()
    }
}
