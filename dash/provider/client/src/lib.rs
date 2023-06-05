use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use dash_api::{
    function::FunctionCrd,
    job::{DashJobCrd, DashJobSpec},
};
use dash_provider::storage::KubernetesStorageClient;
use kube::{
    api::{DeleteParams, ListParams, PostParams},
    core::ObjectMeta,
    Api, Client, ResourceExt,
};
use serde_json::Value;
use vine_rbac::auth::UserSessionRef;

pub(crate) const NAME: &str = "dash-provider-client";

pub struct DashProviderClient<'a> {
    api: Api<DashJobCrd>,
    client: Client,
    session: &'a UserSessionRef,
}

impl<'a> DashProviderClient<'a> {
    pub fn new(client: Client, session: &'a UserSessionRef) -> Self {
        Self {
            api: Api::namespaced(client.clone(), &session.namespace),
            client,
            session,
        }
    }

    pub async fn create(
        &self,
        function_name: &str,
        value: BTreeMap<String, Value>,
    ) -> Result<DashJobCrd> {
        let storage = KubernetesStorageClient {
            namespace: &self.session.namespace,
            kube: &self.client,
        };
        let function = storage.load_function(function_name).await?;
        self.create_raw(&function, value).await
    }

    pub async fn create_raw(
        &self,
        function: &FunctionCrd,
        value: BTreeMap<String, Value>,
    ) -> Result<DashJobCrd> {
        let function_name = function.name_any();
        let job_name = format!(
            "{name}-{uuid}",
            name = function_name,
            uuid = ::uuid::Uuid::new_v4(),
        );
        let data = DashJobCrd {
            metadata: ObjectMeta {
                name: Some(job_name.clone()),
                namespace: Some(self.session.namespace.clone()),
                ..Default::default()
            },
            spec: DashJobSpec {
                value,
                function: function_name.clone(),
            },
            status: None,
        };

        let pp = PostParams {
            dry_run: false,
            field_manager: Some(self::NAME.into()),
        };
        self.api.create(&pp, &data).await.map_err(|error| {
            anyhow!("failed to create job ({function_name} => {job_name}): {error}")
        })
    }

    pub async fn delete(&self, function_name: &str, job_name: &str) -> Result<()> {
        match self.get(function_name, job_name).await? {
            Some(_) => self.force_delete(function_name, job_name).await,
            None => Ok(()),
        }
    }

    async fn force_delete(&self, function_name: &str, job_name: &str) -> Result<()> {
        let dp = DeleteParams::default();
        self.api
            .delete(job_name, &dp)
            .await
            .map(|_| ())
            .map_err(|error| {
                anyhow!("failed to delete job ({function_name} => {job_name}): {error}")
            })
    }

    pub async fn get(&self, function_name: &str, job_name: &str) -> Result<Option<DashJobCrd>> {
        self.api
            .get_opt(job_name)
            .await
            .map_err(|error| anyhow!("failed to find job ({function_name} => {job_name}): {error}"))
            .and_then(|result| match result {
                Some(job) if job.spec.function == function_name => Ok(Some(job)),
                Some(job) => bail!(
                    "unexpected job: expected function name {expected:?}, but given {given:?}",
                    expected = job_name,
                    given = job.spec.function,
                ),
                None => Ok(None),
            })
    }

    pub async fn get_list(&self) -> Result<Vec<DashJobCrd>> {
        let lp = ListParams::default();
        self.api
            .list(&lp)
            .await
            .map(|list| list.items)
            .map_err(|error| anyhow!("failed to list jobs: {error}"))
    }

    pub async fn get_list_with_function_name(
        &self,
        function_name: &str,
    ) -> Result<Vec<DashJobCrd>> {
        let lp = ListParams {
            label_selector: Some(format!(
                "{key}={value}",
                key = DashJobCrd::LABEL_TARGET_FUNCTION,
                value = function_name,
            )),
            ..Default::default()
        };
        self.api
            .list(&lp)
            .await
            .map(|list| list.items)
            .map_err(|error| anyhow!("failed to list jobs ({function_name}): {error}"))
    }

    pub async fn restart(&self, function_name: &str, job_name: &str) -> Result<DashJobCrd> {
        match self.get(function_name, job_name).await? {
            Some(job) => {
                self.force_delete(function_name, job_name).await?;
                self.create(function_name, job.spec.value).await
            }
            None => bail!("no such job: {function_name:?} => {job_name:?}"),
        }
    }
}
