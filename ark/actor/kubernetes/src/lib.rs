mod job_runtime;

pub mod consts {
    use strum::{Display, EnumString};

    pub const FIELD_MANAGER: &str = "ark-actor-kubernetes";

    pub const IMAGE_PULL_SECRET_NAME: &str = "ark-registry";

    pub const LABEL_BUILD_TIMESTAMP: &str = "ark.ulagbulag.io/build-timestamp";
    pub const LABEL_JOB_KIND: &str = "ark.ulagbulag.io/job-kind";
    pub const LABEL_PACKAGE_NAME: &str = "ark.ulagbulag.io/package-name";

    #[derive(Copy, Clone, Debug, Display, EnumString, PartialEq, Eq, Hash)]
    #[strum(serialize_all = "camelCase")]
    pub enum JobKind {
        Add,
        Run,
    }
}

use std::fmt;

use ark_actor_api::{
    args::ActorArgs,
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
    tokio::io,
};
use k8s_openapi::{
    api::core::v1::{Namespace, Pod},
    serde::{de::DeserializeOwned, Serialize},
};
use kube::{
    api::{LogParams, PostParams, WatchParams},
    core::WatchEvent,
    Api, Client, Resource, ResourceExt,
};

pub struct PackageManager {
    app: ApplicationRuntime<self::job_runtime::JobApplicationBuilderFactory>,
    args: ActorArgs,
    repos: RepositoryManager,
}

impl PackageManager {
    pub async fn try_new(args: ActorArgs) -> Result<Self> {
        Ok(Self {
            app: ApplicationRuntime::new(args.container_image_name_prefix.clone()),
            repos: RepositoryManager::try_from_local(&args.repository_home).await?,
            args,
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

    pub fn job_name(name: &str) -> String {
        format!("package-build-{name}")
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
        let package = self.get(name, self.kube.default_namespace()).await?;

        let api = Api::<ArkPackageCrd>::default_namespaced(self.kube.clone());
        exists(&api, &package.name).await
    }

    async fn add(&self, name: &str) -> Result<()> {
        let namespace = self.kube.default_namespace();
        let sync = self.manager.args.sync();

        let mut package = self.get(name, self.kube.default_namespace()).await?;

        let api = Api::<ArkPackageCrd>::default_namespaced(self.kube.clone());
        if exists(&api, &package.name).await? {
            Ok(())
        } else {
            let pp = PostParams {
                field_manager: Some(self::consts::FIELD_MANAGER.into()),
                ..Default::default()
            };
            api.create(&pp, &package.resource).await?;

            if sync {
                self.wait_for_ready(&api, namespace, name, &mut package)
                    .await
            } else {
                Ok(())
            }
        }
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let package = self.get(name, self.kube.default_namespace()).await?;

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

    async fn run(&self, name: &str, command_line_arguments: &[String]) -> Result<()> {
        let namespace = self.kube.default_namespace();
        let sync = self.manager.args.sync();

        let mut package = self.get(name, namespace).await?;

        let api = Api::<ArkPackageCrd>::default_namespaced(self.kube.clone());
        if !exists(&api, &package.name).await? {
            self.manager.args.assert_add_if_not_exists(name)?;

            info!("Starting building package ({namespace}/{name})...");
            self.add(name).await?;
        } else if sync {
            self.wait_for_ready(&api, namespace, name, &mut package)
                .await?;
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
    async fn get(&self, name: &str, namespace: &str) -> Result<Package> {
        let api = Api::<ArkPackageCrd>::namespaced(self.kube.clone(), namespace);
        match api.get_opt(name).await? {
            Some(resource) => Ok(Package {
                name: name.to_string(),
                resource,
            }),
            None => self.manager.repos.get(name).await.map(|mut package| {
                package
                    .resource
                    .metadata
                    .namespace
                    .replace(namespace.into());
                package
            }),
        }
    }

    async fn get_node_name(&self, namespace: &str) -> Result<String> {
        let api: Api<Namespace> = Api::<Namespace>::all(self.kube.clone());
        let namespace = api.get(namespace).await?;
        namespace
            .get_session_ref()
            .map(|session| session.node_name.to_string())
    }

    async fn wait_for_ready(
        &self,
        api: &Api<ArkPackageCrd>,
        namespace: &str,
        name: &str,
        package: &mut Package,
    ) -> Result<()> {
        let is_ready = |resource: &ArkPackageCrd| {
            resource
                .status
                .as_ref()
                .map(|status| status.state == ArkPackageState::Ready)
                .unwrap_or_default()
        };

        if is_ready(&package.resource) {
            Ok(())
        } else {
            {
                let api = Api::<Pod>::namespaced(self.kube.clone(), namespace);
                let name = PackageManager::job_name(name);
                let skip_if_not_exists: bool = false;

                info!("Waiting for building package ({namespace}/{name})...");
                show_logs(&api, namespace, &name, skip_if_not_exists).await?
            }

            let wp = WatchParams {
                label_selector: Some({
                    let key = crate::consts::LABEL_PACKAGE_NAME;
                    format!("{key}={name}")
                }),
                timeout: Some(290),
                ..Default::default()
            };

            info!("Waiting for package to be ready ({namespace}/{name})...");
            match wait_for(api, &wp, is_ready).await? {
                Some(resource) => {
                    package.resource = resource;
                    Ok(())
                }
                None => bail!("failed to find package: {name}"),
            }
        }
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

async fn show_logs(
    api: &Api<Pod>,
    namespace: &str,
    name: &str,
    skip_if_not_exists: bool,
) -> Result<()> {
    async fn get_pod_name(api: &Api<Pod>, name: &str) -> Result<Option<String>> {
        let wp = WatchParams {
            label_selector: Some(format!("job-name={name}")),
            timeout: Some(290),
            ..Default::default()
        };
        let phase_ready = &["Running", "Failed", "Succeeded"];
        let is_running = |pod: &Pod| {
            pod.status
                .as_ref()
                .and_then(|status| status.phase.as_ref())
                .map(|phase| phase_ready.contains(&phase.as_str()))
                .unwrap_or_default()
        };

        wait_for(api, &wp, is_running)
            .await
            .map(|maybe_pod| maybe_pod.map(|pod| pod.name_any()))
    }

    info!("Waiting for a pod ({namespace}/{name})...");
    let pod_name = match get_pod_name(api, name).await {
        Ok(Some(pod_name)) => pod_name,
        Ok(None) => {
            if skip_if_not_exists {
                return Ok(());
            } else {
                bail!("failed to create a pod: {name}")
            }
        }
        Err(e) => {
            return Err(e);
        }
    };

    async fn show_pod_logs(api: &Api<Pod>, pod_name: &str) -> Result<()> {
        let lp = LogParams {
            follow: true,
            pretty: true,
            ..Default::default()
        };
        let mut stream = api.log_stream(pod_name, &lp).await?;
        let mut stdout = io::stdout();

        while let Some(value) = stream.next().await {
            let value = value?;
            io::copy(&mut value.as_ref(), &mut stdout).await?;
        }
        Ok(())
    }

    info!("Getting logs ({namespace}/{name})...");
    show_pod_logs(api, &pod_name).await
}
