mod job_runtime;

pub mod consts {
    pub const FIELD_MANAGER: &str = "ark-actor-kubernetes";

    pub const IMAGE_PULL_SECRET_NAME: &str = "ark-registry";

    pub const LABEL_BUILD_TIMESTAMP: &str = "ark.ulagbulag.io/build-timestamp";
    pub const LABEL_PACKAGE_NAME: &str = "ark.ulagbulag.io/package-name";
}

use std::fmt;

use ark_actor_api::{
    args::{ActorArgs, PackageFlags},
    package::Package,
    repo::RepositoryManager,
    runtime::{ApplicationRuntime, ApplicationRuntimeCtx},
};
use ark_api::{
    package::{ArkPackageCrd, ArkPackageState},
    NamespaceAny,
};
use ipis::{
    async_trait::async_trait,
    core::anyhow::{bail, Result},
    futures::StreamExt,
    log::info,
};
use k8s_openapi::{
    api::core::v1::Namespace,
    serde::{de::DeserializeOwned, Serialize},
};
use kube::{
    api::{PostParams, WatchParams},
    core::WatchEvent,
    Api, Client, Resource, ResourceExt,
};

pub struct PackageManager {
    app: ApplicationRuntime<self::job_runtime::JobApplicationBuilderFactory>,
    flags: PackageFlags,
    repos: RepositoryManager,
}

impl PackageManager {
    pub async fn try_new(args: &ActorArgs) -> Result<Self> {
        Ok(Self {
            app: ApplicationRuntime::new(args.container_image_name_prefix.clone()),
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
            kube: {
                let mut config = ::kube::Config::infer().await?;
                config.read_timeout = None; // disable sync timeout
                config.try_into()?
            },
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

    async fn run(&self, name: &str, args: &[String], sync: bool) -> Result<()> {
        let Self { kube, manager } = self;
        let session = PackageSession { kube, manager };
        session.run(name, args, sync).await
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
                field_manager: Some(self::consts::FIELD_MANAGER.into()),
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

    async fn run(&self, name: &str, command_line_arguments: &[String], sync: bool) -> Result<()> {
        let namespace = self.kube.default_namespace();
        let mut package = self.manager.get(name, namespace).await?;

        let api = Api::<ArkPackageCrd>::default_namespaced(self.kube.clone());
        if !exists(&api, &package.name).await? {
            self.manager.flags.assert_add_if_not_exists(name)?;

            info!("Starting building package ({namespace}/{name})...");
            self.add(name).await?;
        }

        // wait for ready
        {
            let is_ready = |resource: &ArkPackageCrd| {
                resource
                    .status
                    .as_ref()
                    .map(|status| status.state == ArkPackageState::Ready)
                    .unwrap_or_default()
            };

            if !is_ready(&package.resource) {
                info!("Waiting for building package ({namespace}/{name})...");

                let wp = WatchParams {
                    label_selector: Some({
                        let key = crate::consts::LABEL_PACKAGE_NAME;
                        format!("{key}={name}")
                    }),
                    timeout: Some(290),
                    ..Default::default()
                };

                match wait_for(&api, &wp, is_ready).await? {
                    Some(resource) => {
                        package.resource = resource;
                    }
                    None => bail!("failed to find package: {name}"),
                }
            };
        }

        let node_name = self.get_node_name(namespace).await?;

        let args = self::job_runtime::JobApplicationBuilderArgs {
            kube: self.kube,
            package: &package,
        };
        let ctx = ApplicationRuntimeCtx {
            namespace,
            node_name: Some(&node_name),
            package: &package,
            command_line_arguments,
            sync,
        };
        self.manager.app.spawn(args, ctx).await
    }
}

impl<'kube, 'manager> PackageSession<'kube, 'manager> {
    async fn get_node_name(&self, namespace: &str) -> Result<String> {
        let api: Api<Namespace> = Api::<Namespace>::all(self.kube.clone());
        let namespace = api.get(namespace).await?;
        namespace
            .get_session_ref()
            .map(|session| session.node_name.to_string())
    }
}

async fn exists(api: &Api<ArkPackageCrd>, name: &str) -> Result<bool> {
    api.get_opt(name)
        .await
        .map(|object| object.is_some())
        .map_err(Into::into)
}

async fn wait_for<K>(api: &Api<K>, wp: &WatchParams, test: impl Fn(&K) -> bool) -> Result<Option<K>>
where
    K: Clone + fmt::Debug + Serialize + DeserializeOwned + ResourceExt,
    <K as Resource>::DynamicType: Default,
{
    let mut stream = api.watch(wp, "0").await?.boxed();

    while let Some(event) = stream.next().await {
        match event? {
            WatchEvent::Added(object) | WatchEvent::Modified(object) => {
                if test(&object) {
                    return Ok(Some(object));
                } else {
                    continue;
                }
            }
            WatchEvent::Deleted(_) => return Ok(None),
            WatchEvent::Bookmark(_) | WatchEvent::Error(_) => continue,
        }
    }
    Ok(None)
}
