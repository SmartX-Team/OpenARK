use std::{borrow::Cow, collections::BTreeMap, fmt, io::Write};

use anyhow::{anyhow, bail, Result};
use dash_api::{
    model::{ModelCrd, ModelCustomResourceDefinitionRefSpec},
    storage::object::{
        ModelStorageObjectBorrowedSecretRefSpec, ModelStorageObjectBorrowedSpec,
        ModelStorageObjectClonedSpec, ModelStorageObjectDeletionPolicy,
        ModelStorageObjectOwnedSpec, ModelStorageObjectSpec,
    },
};
use futures::{future::try_join_all, TryFutureExt};
use k8s_openapi::api::{
    core::v1::Secret,
    networking::v1::{
        HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
        IngressServiceBackend, IngressSpec, ServiceBackendPort,
    },
};
use kube::{
    api::PostParams,
    core::{DynamicObject, ObjectMeta, TypeMeta},
    Api, Client, ResourceExt,
};
use minio::s3::{
    args::{BucketExistsArgs, GetObjectArgs, ListObjectsV2Args, MakeBucketArgs},
    creds::{Credentials, Provider, StaticProvider},
    http::BaseUrl,
};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use reqwest::{Method, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

pub struct ObjectStorageClient {
    base_url: BaseUrl,
    endpoint: Url,
    name: String,
    provider: StaticProvider,
    read_only: bool,
}

impl<'model> ObjectStorageClient {
    pub async fn try_new(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectSpec,
    ) -> Result<Self> {
        Self::load_storage_provider(kube, namespace, name, storage)
            .await
            .map_err(|error| anyhow!("failed to load object storage provider: {error}"))
    }

    async fn load_storage_provider(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectSpec,
    ) -> Result<Self> {
        match storage {
            ModelStorageObjectSpec::Borrowed(storage) => {
                Self::load_storage_provider_by_borrowed(kube, namespace, name, storage).await
            }
            ModelStorageObjectSpec::Cloned(storage) => {
                Self::load_storage_provider_by_cloned(kube, namespace, name, storage).await
            }
            ModelStorageObjectSpec::Owned(storage) => {
                Self::load_storage_provider_by_owned(kube, namespace, name, storage).await
            }
        }
    }

    async fn load_storage_provider_by_borrowed(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectBorrowedSpec,
    ) -> Result<Self> {
        // TODO: Ingress 리다이렉팅 구현하기
        // TODO: cloned, owned 의 경우에는 이를 무시하기 (=기존 함수를 별도 함수로 분리)

        let ModelStorageObjectBorrowedSpec {
            endpoint,
            secret_ref:
                ModelStorageObjectBorrowedSecretRefSpec {
                    map_access_key,
                    map_secret_key,
                    name: secret_name,
                },
            read_only,
        } = storage;

        let mut secret = match {
            let api = Api::<Secret>::namespaced(kube.clone(), namespace);
            api.get_opt(secret_name).await?
        } {
            Some(secret) => secret,
            None => bail!("no such secret: {secret_name}"),
        };

        let mut get_secret_data =
            |key: &str| match secret.data.as_mut().and_then(|data| data.remove(key)) {
                Some(value) => String::from_utf8(value.0).map_err(|error| {
                    anyhow!("failed to parse secret key ({secret_name}/{key}): {error}")
                }),
                None => bail!("no such secret key: {secret_name}/{key}"),
            };

        Ok(Self {
            base_url: BaseUrl::from_string(endpoint.to_string())?,
            endpoint: endpoint.0.clone(),
            name: name.to_string(),
            provider: StaticProvider::new(
                &get_secret_data(map_access_key)?,
                &get_secret_data(map_secret_key)?,
                None,
            ),
            read_only: *read_only,
        })
    }

    async fn load_storage_provider_by_cloned(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectClonedSpec,
    ) -> Result<Self> {
        let borrowed =
            Self::load_storage_provider_by_borrowed(kube, namespace, name, &storage.borrowed)
                .await?;
        let owned =
            Self::load_storage_provider_by_owned(kube, namespace, name, &storage.owned).await?;

        let admin = MinioAdminClient { storage: &borrowed };
        // TODO: verify and join endpoint
        if !admin.is_site_replication_enabled().await? {
            admin.add_site_replication(&owned).await?;
        }
        Ok(owned)
    }

    async fn load_storage_provider_by_owned(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectOwnedSpec,
    ) -> Result<Self> {
        let storage = Self::create_or_get_storage(kube, namespace, name, storage).await?;
        Self::load_storage_provider_by_borrowed(kube, namespace, name, &storage).await
    }

    async fn create_or_get_storage(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectOwnedSpec,
    ) -> Result<ModelStorageObjectBorrowedSpec> {
        let ModelStorageObjectOwnedSpec {
            deletion_policy: ModelStorageObjectDeletionPolicy::Retain,
            resources,
        } = storage;

        async fn get_or_create<K, Data>(
            api: &Api<K>,
            pp: &PostParams,
            kind: &str,
            name: &str,
            data: Data,
        ) -> Result<K>
        where
            Data: FnOnce() -> K,
            K: Clone + fmt::Debug + Serialize + DeserializeOwned,
        {
            match api.get_opt(name).await {
                Ok(Some(value)) => Ok(value),
                Ok(None) => api
                    .create(pp, &data())
                    .await
                    .map_err(|error| anyhow!("failed to create {kind} ({name}): {error}")),
                Err(error) => bail!("failed to get {kind} ({name}): {error}"),
            }
        }

        async fn get_latest_minio_image() -> Result<String> {
            // TODO: to be implemented
            Ok("minio/minio:RELEASE.2023-06-09T07-32-12Z".into())
        }

        fn random_string(n: usize) -> String {
            let mut rng = thread_rng();
            (0..n).map(|_| rng.sample(Alphanumeric) as char).collect()
        }

        let api_secret = Api::<Secret>::namespaced(kube.clone(), namespace);

        let pp = PostParams {
            dry_run: false,
            field_manager: Some(crate::NAME.into()),
        };

        let tenant_name = format!("object-storage-{name}");
        let labels = {
            let mut map: BTreeMap<String, String> = BTreeMap::default();
            map.insert("v1.min.io/tenant".into(), tenant_name.clone());
            map
        };

        let secret_env_configuration = {
            let name = format!("{tenant_name}-env-configuration");
            get_or_create(&api_secret, &pp, "secret", &name, || Secret {
                metadata: ObjectMeta {
                    labels: Some(labels.clone()),
                    name: Some(name.clone()),
                    namespace: Some(namespace.to_string()),
                    ..Default::default()
                },
                immutable: Some(false),
                string_data: Some({
                    let mut map: BTreeMap<String, String> = BTreeMap::default();
                    map.insert(
                        "config.env".into(),
                        format!(
                            r#"
export MINIO_BROWSER="on"
export MINIO_STORAGE_CLASS_STANDARD="EC:4"
export MINIO_ROOT_USER="{username}"
export MINIO_ROOT_PASSWORD="{password}"
"#,
                            username = random_string(16),
                            password = random_string(32),
                        ),
                    );
                    map
                }),
                ..Default::default()
            })
            .await?
        };

        let secret_creds = {
            let name = format!("{tenant_name}-secret");
            get_or_create(&api_secret, &pp, "secret", &name, || Secret {
                metadata: ObjectMeta {
                    labels: Some(labels.clone()),
                    name: Some(name.clone()),
                    namespace: Some(namespace.to_string()),
                    ..Default::default()
                },
                immutable: Some(true),
                string_data: Some({
                    let mut map: BTreeMap<String, String> = BTreeMap::default();
                    map.insert("accesskey".into(), Default::default());
                    map.insert("secretkey".into(), Default::default());
                    map
                }),
                ..Default::default()
            })
            .await?
        };

        let secret_user_0 = {
            let name = format!("{tenant_name}-user-0");
            get_or_create(&api_secret, &pp, "secret", &name, || Secret {
                metadata: ObjectMeta {
                    labels: Some(labels.clone()),
                    name: Some(name.clone()),
                    namespace: Some(namespace.to_string()),
                    ..Default::default()
                },
                immutable: Some(true),
                string_data: Some({
                    let mut map: BTreeMap<String, String> = BTreeMap::default();
                    map.insert("CONSOLE_ACCESS_KEY".into(), random_string(16));
                    map.insert("CONSOLE_SECRET_KEY".into(), random_string(32));
                    map
                }),
                ..Default::default()
            })
            .await?
        };

        {
            let name = "object-storage";
            let api_ingress = Api::<Ingress>::namespaced(kube.clone(), namespace);
            get_or_create(&api_ingress, &pp, "ingress", name, || Ingress {
                metadata: ObjectMeta {
                    annotations: Some({
                        let mut map = BTreeMap::default();
                        map.insert(
                            "cert-manager.io/cluster-issuer".into(),
                            "ingress-nginx-controller.vine.svc.ops.openark".into(),
                        );
                        map.insert(
                            "kubernetes.io/ingress.class".into(),
                            "ingress-nginx-controller.vine.svc.ops.openark".into(),
                        );
                        map.insert(
                            "nginx.ingress.kubernetes.io/proxy-read-timeout".into(),
                            "3600".into(),
                        );
                        map.insert(
                            "nginx.ingress.kubernetes.io/proxy-send-timeout".into(),
                            "3600".into(),
                        );
                        map.insert(
                            "nginx.ingress.kubernetes.io/rewrite-target".into(),
                            "/$2".into(),
                        );
                        map.insert("vine.ulagbulag.io/is-service".into(), "true".into());
                        map.insert("vine.ulagbulag.io/is-service-public".into(), "true".into());
                        map.insert("vine.ulagbulag.io/is-service-system".into(), "true".into());
                        map.insert(
                            "vine.ulagbulag.io/service-kind".into(),
                            "S3 Endpoint".into(),
                        );
                        map
                    }),
                    labels: Some(labels.clone()),
                    name: Some(name.to_string()),
                    namespace: Some(namespace.to_string()),
                    ..Default::default()
                },
                spec: Some(IngressSpec {
                    rules: Some(vec![IngressRule {
                        host: Some("ingress-nginx-controller.vine.svc.ops.openark".into()),
                        http: Some(HTTPIngressRuleValue {
                            paths: vec![HTTPIngressPath {
                                path: Some(format!("/data/s3/{namespace}(/|$)(.*)")),
                                path_type: "Prefix".into(),
                                backend: IngressBackend {
                                    service: Some(IngressServiceBackend {
                                        name: "minio".into(),
                                        port: Some(ServiceBackendPort {
                                            name: Some("http-minio".into()),
                                            ..Default::default()
                                        }),
                                    }),
                                    ..Default::default()
                                },
                            }],
                        }),
                    }]),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await?
        };

        {
            let name = &tenant_name;
            let pool_name = "pool-0";

            let minio_image = get_latest_minio_image().await?;

            let api_tenant = {
                let client = super::kubernetes::KubernetesStorageClient { namespace, kube };
                let spec = ModelCustomResourceDefinitionRefSpec {
                    name: "tenants.minio.min.io/v2".into(),
                };
                client.api_custom_resource(&spec, None).await?
            };
            get_or_create(&api_tenant, &pp, "tenant", name, || DynamicObject {
                types: Some(TypeMeta {
                    api_version: "minio.min.io/v2".into(),
                    kind: "Tenant".into(),
                }),
                metadata: ObjectMeta {
                    labels: Some(labels),
                    name: Some(name.clone()),
                    namespace: Some(namespace.to_string()),
                    ..Default::default()
                },
                data: json!({
                    "spec": {
                        "configuration": {
                            "name": secret_env_configuration.name_any(),
                        },
                        "credsSecret": {
                            "name": secret_creds.name_any(),
                        },
                        "exposeServices": {},
                        "image": minio_image,
                        "imagePullSecret": {},
                        "mountPath": "/export",
                        "pools": [
                            {
                                "affinity": {
                                    "podAntiAffinity": {
                                        "requiredDuringSchedulingIgnoredDuringExecution": [
                                            {
                                                "labelSelector": {
                                                    "matchExpressions": [
                                                        {
                                                            "key": "v1.min.io/tenant",
                                                            "operator": "In",
                                                            "values": [
                                                                name,
                                                            ],
                                                        },
                                                        {
                                                            "key": "v1.min.io/pool",
                                                            "operator": "In",
                                                            "values": [
                                                                pool_name,
                                                            ],
                                                        },
                                                    ],
                                                },
                                                "topologyKey": "kubernetes.io/hostname",
                                            },
                                        ],
                                    },
                                },
                                "name": pool_name,
                                "resources": {
                                    "requests": {
                                        "cpu": "16",
                                        "memory": "31Gi",
                                    },
                                },
                                "runtimeClassName": "",
                                "servers": 4,
                                "volumeClaimTemplate": {
                                    "metadata": {
                                        "name": "data",
                                    },
                                    "spec": {
                                        "accessModes": [
                                            "ReadWriteOnce",
                                        ],
                                        "resources": resources,
                                        "storageClassName": "ceph-block",
                                    },
                                    "status": {},
                                },
                                "volumesPerServer": 4,
                            },
                        ],
                        "requestAutoCert": false,
                        "users": [
                            {
                                "name": secret_user_0.name_any(),
                            },
                        ],
                    },
                }),
            })
            .await?
        };

        Ok(ModelStorageObjectBorrowedSpec {
            // TODO: use real cluster domain name (not ops.openark.)
            endpoint: format!("http://minio.{namespace}.svc.ops.openark/").parse()?,
            read_only: false,
            secret_ref: ModelStorageObjectBorrowedSecretRefSpec {
                map_access_key: "CONSOLE_ACCESS_KEY".into(),
                map_secret_key: "CONSOLE_SECRET_KEY".into(),
                name: secret_user_0.name_any(),
            },
        })
    }
}

impl ObjectStorageClient {
    fn get_client(&self) -> ::minio::s3::client::Client<'_> {
        let mut client =
            ::minio::s3::client::Client::new(self.base_url.clone(), Some(&self.provider));
        client.ignore_cert_check = true;
        client
    }

    pub fn get_session<'model>(&self, model: &'model ModelCrd) -> ObjectStorageSession<'_, 'model> {
        ObjectStorageSession {
            client: self.get_client(),
            model,
        }
    }
}
pub struct ObjectStorageSession<'client, 'model> {
    client: ::minio::s3::client::Client<'client>,
    model: &'model ModelCrd,
}

impl<'client, 'model> ObjectStorageSession<'client, 'model> {
    fn get_bucket_name(&self) -> String {
        self.model.name_any()
    }

    async fn is_bucket_exists(&self) -> Result<bool> {
        let bucket_name = self.get_bucket_name();
        self.client
            .bucket_exists(&BucketExistsArgs::new(&bucket_name)?)
            .await
            .map_err(|error| anyhow!("failed to check bucket ({bucket_name}): {error}"))
    }

    pub async fn get(&self, ref_name: &str) -> Result<Option<Value>> {
        let bucket_name = self.get_bucket_name();
        let args = GetObjectArgs::new(&bucket_name, ref_name)?;

        match self.client.get_object(&args).await {
            Ok(response) => response.json().await.map_err(|error| {
                anyhow!("failed to parse object ({bucket_name}/{ref_name}): {error}")
            }),
            Err(error) => match &error {
                ::minio::s3::error::Error::S3Error(response) if response.code == "NoSuchKey" => {
                    Ok(None)
                }
                _ => bail!("failed to get object ({bucket_name}/{ref_name}): {error}"),
            },
        }
    }

    pub async fn get_list(&self) -> Result<Vec<Value>> {
        const LIMIT: u16 = 30;

        let bucket_name = self.get_bucket_name();
        let mut args = ListObjectsV2Args::new(&bucket_name)?;
        args.max_keys = Some(LIMIT);

        match self.client.list_objects_v2(&args).await {
            Ok(response) => try_join_all(
                response
                    .contents
                    .into_iter()
                    .map(|item| async move { self.get(&item.name).await }),
            )
            .await
            .map(|values| values.into_iter().flatten().collect())
            .map_err(|error| anyhow!("failed to list object ({bucket_name}): {error}")),
            Err(error) => bail!("failed to list object ({bucket_name}): {error}"),
        }
    }

    pub async fn create_bucket(&self) -> Result<()> {
        if self.is_bucket_exists().await? {
            return Ok(());
        }

        let bucket_name = self.get_bucket_name();

        let args = MakeBucketArgs::new(&bucket_name)?;
        self.client
            .make_bucket(&args)
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to create a bucket ({bucket_name}): {error}"))
    }
}

struct MinioAdminClient<'storage> {
    storage: &'storage ObjectStorageClient,
}

impl<'storage> MinioAdminClient<'storage> {
    async fn add_site_replication(&self, target: &ObjectStorageClient) -> Result<()> {
        let origin_creds = self.storage.provider.fetch();
        let target_creds = target.provider.fetch();

        let sites = json! ([
            {
                "name": format!(
                    "{origin},{target},{name}",
                    name = &self.storage.name,
                    origin = &self.storage.endpoint,
                    target = &target.endpoint,
                ),
                "endpoints": &self.storage.endpoint,
                "accessKey": origin_creds.access_key,
                "secretKey": origin_creds.secret_key,
            },
            {
                "name": format!(
                    "{target},{origin},{name}",
                    name = &target.name,
                    origin = &self.storage.endpoint,
                    target = &target.endpoint,
                ),
                "endpoints": &target.endpoint,
                "accessKey": target_creds.access_key,
                "secretKey": target_creds.secret_key,
            },
        ]);
        let ciphertext = self.encrypt_data(Some(&origin_creds), &sites)?;

        self.execute(
            Method::PUT,
            "/admin/v3/site-replication/add",
            Some(&ciphertext),
        )
        .await
        .map(|_| ())
        .map_err(|error| {
            anyhow!(
                "failed to add site replication ({name}: {origin} => {target}): {error}",
                name = &self.storage.name,
                origin = &self.storage.endpoint,
                target = &target.endpoint,
            )
        })
    }

    async fn is_site_replication_enabled(&self) -> Result<bool> {
        self.execute(Method::GET, "/admin/v3/site-replication/info", None)
            .and_then(|resp| async move {
                #[derive(Deserialize)]
                struct Data {
                    enabled: bool,
                }

                let data: Data = resp.json().await?;
                Ok(data.enabled)
            })
            .await
            .map_err(|error| {
                anyhow!(
                    "failed to check site replication ({name}: {origin}): {error}",
                    name = &self.storage.name,
                    origin = &self.storage.endpoint,
                )
            })
    }

    async fn execute(
        &self,
        method: Method,
        base_url: &str,
        data: Option<&[u8]>,
    ) -> Result<::reqwest::Response, ::minio::s3::error::Error> {
        self.storage
            .get_client()
            .execute(
                method,
                &Default::default(),
                &mut Default::default(),
                &Default::default(),
                Some("minio"),
                Some(base_url),
                data,
            )
            .await
    }

    fn encrypt_data<T>(
        &self,
        creds: Option<&Credentials>,
        data: &T,
    ) -> Result<Vec<u8>, ::minio::s3::error::Error>
    where
        T: ?Sized + Serialize,
    {
        let creds = creds
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(self.storage.provider.fetch()));
        let data = ::serde_json::to_vec(&data)?;

        // FIXME: use CryptoRng instead!
        let mut rng = thread_rng();

        let mut salt = [0u8; 32];
        rng.fill(&mut salt);

        const ID: u8 = 0x01; // argon2idChaCHa20Poly1305
        let mut key = [0u8; 32];
        ::argon2::Argon2::new(
            Default::default(),
            Default::default(),
            ::argon2::Params::new(64 * 1024, 1, 4, Some(key.len())).unwrap(),
        )
        .hash_password_into(creds.secret_key.as_bytes(), &salt, &mut key)
        .unwrap();

        let mut nonce = [0u8; 8];
        rng.fill(&mut nonce);

        let mut encrypted_data = {
            // Load your secret keys from a secure location or derive
            // them using a secure (password-based) key-derivation-function, like Argon2id.
            // Obviously, don't use this all-zeros key for anything real.
            let key = ::sio::Key::<::sio::CHACHA20_POLY1305>::new(key);

            // Make sure you use an unique key-nonce combination!
            // Reusing a nonce value for the same secret key breaks
            // the security of the encryption algorithm.
            let nonce = ::sio::Nonce::new(nonce);

            // You must be able to re-generate this aad to decrypt
            // the ciphertext again. Usually, it's stored together with
            // the encrypted data.
            let aad = ::sio::Aad::empty();

            let mut buf = Vec::default(); // Store the ciphertext in memory.
            let mut writer = ::sio::EncWriter::new(&mut buf, &key, nonce, aad);

            writer.write_all(&data)?;
            writer.close()?; // Complete the encryption process explicitly.
            buf
        };

        // Prefix the ciphertext with salt, AEAD ID and nonce
        let mut ciphertext = Vec::new();
        ciphertext.append(&mut salt.to_vec());
        ciphertext.push(ID);
        ciphertext.append(&mut nonce.to_vec());
        ciphertext.append(&mut encrypted_data);
        Ok(ciphertext)
    }
}
