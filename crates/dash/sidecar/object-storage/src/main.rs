use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Result;
use clap::Parser;
use dash_api::{
    model_storage_binding::{ModelStorageBindingCrd, ModelStorageBindingState},
    storage::ModelStorageKind,
};
use futures::{lock::Mutex, TryStreamExt};
use kube::{
    runtime::watcher::{watcher, Config, Error, Event},
    Api, Client, ResourceExt,
};
use tokio::{fs::OpenOptions, io::AsyncWriteExt};
use tracing::{error, info};

#[derive(Parser)]
struct Args {
    #[arg(short, long, env = "NAMESPACE", value_name = "NAME")]
    namespace: Option<String>,

    #[arg(
        short,
        long,
        env = "NGINX_CONF_PATH",
        value_name = "PATH",
        default_value = Args::default_path(),
    )]
    path: PathBuf,
}

impl Args {
    const fn default_path() -> &'static str {
        "/etc/nginx/nginx.conf.new"
    }
}

#[::tokio::main]
async fn main() {
    ::ark_core::tracer::init_once();

    match try_main().await {
        Ok(()) => (),
        Err(error) => error!("{error}"),
    }
}

async fn try_main() -> Result<()> {
    let args = Args::try_parse()?;

    let kube = Client::try_default().await?;
    let namespace = args
        .namespace
        .clone()
        .unwrap_or_else(|| kube.default_namespace().into());
    let default_namespace = || namespace.clone();

    let ctx = Context::default();
    let handle_event = |e| handle_event(&args, &ctx, default_namespace, e);

    let api = Api::<ModelStorageBindingCrd>::namespaced(kube, &namespace);

    watcher(api, Config::default())
        .try_for_each(handle_event)
        .await
        .map_err(Into::into)
}

#[derive(Default)]
struct Context {
    data: Mutex<BTreeMap<String, ModelStorageBindingCrd>>,
}

async fn handle_event(
    args: &Args,
    ctx: &Context,
    default_namespace: impl Copy + Fn() -> String,
    event: Event<ModelStorageBindingCrd>,
) -> Result<(), Error> {
    let Args { namespace: _, path } = args;
    let Context { data } = ctx;

    {
        let mut data = data.lock().await;
        match event {
            Event::Applied(cr) => {
                let name = cr.name_any();
                info!("Applying {name} binding...");
                data.insert(name, cr);
            }
            Event::Deleted(cr) => {
                let name = cr.name_any();
                info!("Deleting {name} binding...");
                data.remove(&name);
            }
            Event::Restarted(crs) => {
                info!("Applying {len} bindings...", len = crs.len());
                data.extend(crs.into_iter().map(|cr| (cr.name_any(), cr)))
            }
        }
    }

    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(path)
            .await
            .map_err(handle_error)?;
        {
            let data = data.lock().await;
            file.set_len(0).await.map_err(handle_error)?;
            file.write_all(build_nginx_conf(default_namespace, &data).as_bytes())
                .await
                .map_err(handle_error)?;
        }
    }
    Ok(())
}

fn handle_error(error: ::std::io::Error) -> Error {
    Error::WatchFailed(::kube::Error::Service(error.into()))
}

fn build_nginx_conf(
    default_namespace: impl Copy + Fn() -> String,
    data: &BTreeMap<String, ModelStorageBindingCrd>,
) -> String {
    let routers = data
        .iter()
        .filter_map(|(name, cr)| {
            let status = cr.status.as_ref()?;

            let target = &status.storage_target.as_ref()?.kind;
            if status.state != ModelStorageBindingState::Ready
                || target.to_kind() != ModelStorageKind::ObjectStorage
            {
                return None;
            }

            let namespace = cr.namespace().unwrap_or_else(default_namespace);
            let borrowed = status
                .storage_sync_policy
                .map(|policy| policy.is_none())
                .unwrap_or_default();

            let kind = if borrowed {
                &status.storage_source.as_ref()?.kind
            } else {
                &status.storage_target.as_ref()?.kind
            };

            let mut path = kind.endpoint(&namespace)?.to_string();
            if path.ends_with('/') {
                path.pop();
            }

            Some(format!(
                r#"location /{name} {{
            proxy_pass {path};
        }}"#
            ))
        })
        .collect::<Vec<_>>()
        .join("\n        ");

    format!(
        r#"
events {{
    # worker_connections 1024;
}}

http {{
    server {{
        listen 80;

        # Enable header forwarding
        proxy_pass_request_headers on;

        # Disable NGINX headers
        proxy_set_header Host $http_host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Disable unneeded headers
        proxy_hide_header X-Powered-By;
        proxy_hide_header Server;

        # Enable dataset routing
        location / {{
            proxy_pass http://minio;
        }}
        {routers}
    }}
}}"#
    )
}
