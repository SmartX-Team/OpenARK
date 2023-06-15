use anyhow::{anyhow, bail, Result};
use dash_api::{
    model::ModelCrd,
    storage::object::{
        ModelStorageObjectBorrowedSecretRefSpec, ModelStorageObjectBorrowedSpec,
        ModelStorageObjectOwnedSpec, ModelStorageObjectSpec,
    },
};
use futures::future::try_join_all;
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client, ResourceExt};
use minio::s3::{
    args::{BucketExistsArgs, GetObjectArgs, ListObjectsV2Args, MakeBucketArgs},
    creds::StaticProvider,
    http::BaseUrl,
};
use serde_json::Value;

pub struct ObjectStorageClient {
    base_url: BaseUrl,
    provider: StaticProvider,
    read_only: bool,
}

impl<'model> ObjectStorageClient {
    pub async fn try_new(
        kube: &Client,
        namespace: &str,
        storage: &ModelStorageObjectSpec,
    ) -> Result<Self> {
        Self::load_storage_provider(kube, namespace, storage)
            .await
            .map_err(|error| anyhow!("failed to load object storage provider: {error}"))
    }

    async fn load_storage_provider(
        kube: &Client,
        namespace: &str,
        storage: &ModelStorageObjectSpec,
    ) -> Result<Self> {
        match storage {
            ModelStorageObjectSpec::Borrowed(storage) => {
                Self::load_storage_provider_by_borrowed(kube, namespace, storage).await
            }
            ModelStorageObjectSpec::Owned(storage) => {
                Self::load_storage_provider_by_owned(kube, namespace, storage).await
            }
        }
    }

    async fn load_storage_provider_by_borrowed(
        kube: &Client,
        namespace: &str,
        storage: &ModelStorageObjectBorrowedSpec,
    ) -> Result<Self> {
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
            provider: StaticProvider::new(
                &get_secret_data(map_access_key)?,
                &get_secret_data(map_secret_key)?,
                None,
            ),
            read_only: *read_only,
        })
    }

    async fn load_storage_provider_by_owned(
        kube: &Client,
        namespace: &str,
        storage: &ModelStorageObjectOwnedSpec,
    ) -> Result<Self> {
        let storage = Self::create_or_get_storage(kube, namespace, storage).await?;
        Self::load_storage_provider_by_borrowed(kube, namespace, &storage).await
    }

    async fn create_or_get_storage(
        kube: &Client,
        namespace: &str,
        storage: &ModelStorageObjectOwnedSpec,
    ) -> Result<ModelStorageObjectBorrowedSpec> {
        let ModelStorageObjectOwnedSpec {
            deletion_policy,
            resources,
        } = storage;

        // TODO: to be implemented
        bail!("creating object storage is not supported yet!");

        let storage = ModelStorageObjectBorrowedSpec {
            endpoint: todo!(),
            read_only: false,
            secret_ref: todo!(),
        };
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
            .map(|values| values.into_iter().filter_map(|value| value).collect())
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
