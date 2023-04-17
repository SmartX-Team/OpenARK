mod job_runtime;

use ark_actor_api::{
    args::{ActorArgs, PackageFlags},
    package::Package,
    repo::RepositoryManager,
};
use ark_api::package::ArkPackageCrd;
use ipis::{async_trait::async_trait, core::anyhow::Result};
use kube::{api::PostParams, Api, Client};

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

    async fn get(&self, name: &str, namespace: &str) -> Result<Package> {
        self.repos.get(name).await.map(|mut package| {
            package
                .resource
                .metadata
                .namespace
                .replace(namespace.into());
            package
        })
    }
}

pub struct PackageSessionOwned {
    kube: Client,
    manager: PackageManager,
}

#[async_trait]
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

#[async_trait]
impl<'kube, 'manager> ::ark_actor_api::PackageManager for PackageSession<'kube, 'manager> {
    async fn exists(&self, name: &str) -> Result<bool> {
        let package = self
            .manager
            .get(name, self.kube.default_namespace())
            .await?;

        let api = Api::<ArkPackageCrd>::default_namespaced(self.kube.clone());
        exists(&api, &package.name).await
    }

    async fn add(&self, name: &str) -> Result<()> {
        let package = self
            .manager
            .get(name, self.kube.default_namespace())
            .await?;

        let api = Api::<ArkPackageCrd>::default_namespaced(self.kube.clone());
        if exists(&api, &package.name).await? {
            Ok(())
        } else {
            let pp = PostParams {
                field_manager: Some(FIELD_MANAGER.into()),
                ..Default::default()
            };
            api.create(&pp, &package.resource)
                .await
                .map(|_| ())
                .map_err(Into::into)
        }
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let package = self
            .manager
            .get(name, self.kube.default_namespace())
            .await?;

        let api = Api::<ArkPackageCrd>::default_namespaced(self.kube.clone());
        if exists(&api, &package.name).await? {
            let dp = Default::default();
            api.delete(&package.name, &dp)
                .await
                .map(|_| ())
                .map_err(Into::into)
        } else {
            Ok(())
        }
    }

    async fn run(&self, name: &str, args: &[String]) -> Result<()> {
        let package = self
            .manager
            .get(name, self.kube.default_namespace())
            .await?;

        let api = Api::<ArkPackageCrd>::default_namespaced(self.kube.clone());
        if !exists(&api, &package.name).await? {
            self.manager.flags.assert_add_if_not_exists(name)?;
            self.add(name).await?;
        }

        let builder = self::job_runtime::JobRuntimeBuilder {
            args,
            kube: self.kube,
            package: &package,
        };
        builder.spawn().await
    }
}

async fn exists(api: &Api<ArkPackageCrd>, name: &str) -> Result<bool> {
    api.get_opt(name)
        .await
        .map(|object| object.is_some())
        .map_err(Into::into)
}

const FIELD_MANAGER: &str = "ark-actor-kubernetes";
