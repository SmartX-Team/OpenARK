use std::error::Error;

use anyhow::{anyhow, Result};
use ark_api::SessionRef;
use ark_core::result::Result as SessionResult;
use dash_api::{function::FunctionCrd, job::DashJobCrd, model::ModelCrd};
use dash_provider_api::job::Payload;
use reqwest::{Client, Method, Url};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use vine_api::user_session::{UserSessionCommandBatch, UserSessionRef};

#[derive(Clone, Debug)]
pub struct DashClient {
    client: Client,
    host: Url,
    namespace: Option<String>,
}

impl DashClient {
    pub fn new(client: Client, host: Url, namespace: impl Into<Option<String>>) -> Self {
        Self {
            client,
            host,
            namespace: namespace.into(),
        }
    }

    pub fn with_host<Host>(host: Host, namespace: impl Into<Option<String>>) -> Result<Self>
    where
        Host: TryInto<Url>,
        <Host as TryInto<Url>>::Error: 'static + Send + Sync + Error,
    {
        host.try_into()
            .map(|host| Self::new(Default::default(), host, namespace))
            .map_err(Into::into)
    }

    pub fn to_namespaced(&self, namespace: impl Into<Option<String>>) -> Self {
        Self {
            client: self.client.clone(),
            host: self.host.clone(),
            namespace: namespace.into(),
        }
    }
}

impl DashClient {
    pub async fn get_function(&self, name: &str) -> Result<FunctionCrd> {
        self.get(format!("/function/{name}/")).await
    }

    pub async fn get_function_list(&self) -> Result<ObjectRef> {
        self.get("/function/").await
    }
}

impl DashClient {
    pub async fn delete_job(&self, function_name: &str, job_name: &str) -> Result<()> {
        self.delete(format!("/function/{function_name}/job/{job_name}/"))
            .await
    }

    pub async fn get_job(&self, function_name: &str, job_name: &str) -> Result<Option<DashJobCrd>> {
        self.get(format!("/function/{function_name}/job/{job_name}/"))
            .await
    }

    pub async fn get_job_list(&self) -> Result<Vec<DashJobCrd>> {
        self.get("/job/").await
    }

    pub async fn get_job_list_with_function_name(
        &self,
        function_name: &str,
    ) -> Result<Vec<DashJobCrd>> {
        self.get(format!("/function/{function_name}/job/")).await
    }

    pub async fn post_job(&self, function_name: &str, value: &Value) -> Result<DashJobCrd> {
        self.post(format!("/function/{function_name}/job/"), Some(value))
            .await
    }

    pub async fn post_job_batch(&self, payload: &[Payload<&Value>]) -> Result<Vec<DashJobCrd>> {
        self.post("/batch/job/", Some(payload)).await
    }

    pub async fn restart_job(&self, function_name: &str, job_name: &str) -> Result<DashJobCrd> {
        self.post(
            format!("/function/{function_name}/job/{job_name}/restart/"),
            Option::<&()>::None,
        )
        .await
    }
}

impl DashClient {
    pub async fn get_model(&self, name: &str) -> Result<ModelCrd> {
        self.get(format!("/model/{name}/")).await
    }

    pub async fn get_model_function_list(&self, name: &str) -> Result<Vec<FunctionCrd>> {
        self.get(format!("/model/{name}/function/")).await
    }

    pub async fn get_model_list(&self) -> Result<ObjectRef> {
        self.get("/model/").await
    }

    pub async fn get_model_item(&self, name: &str, item: &str) -> Result<Value> {
        self.get(format!("/model/{name}/item/{item}/")).await
    }

    pub async fn get_model_item_list(&self, name: &str) -> Result<Vec<Value>> {
        self.get(format!("/model/{name}/item/")).await
    }
}

impl DashClient {
    pub async fn get_user(&self) -> Result<UserSessionRef> {
        self.get("/user/").await
    }

    pub async fn get_user_session_list(&self) -> Result<Vec<SessionRef<'static>>> {
        self.get("/batch/user/session/").await
    }

    pub async fn post_user_exec<T>(&self, command: &[T]) -> Result<()>
    where
        T: AsRef<str> + Serialize,
    {
        self.post("/user/desktop/exec/", Some(command)).await
    }

    pub async fn post_user_exec_broadcast<Command, UserName>(
        &self,
        command: &UserSessionCommandBatch<&[Command], &[UserName]>,
    ) -> Result<()>
    where
        Command: AsRef<str> + Serialize,
        UserName: AsRef<str> + Serialize,
    {
        self.post("/batch/user/desktop/exec/broadcast/", Some(command))
            .await
    }
}

impl DashClient {
    async fn delete<Res>(&self, path: impl AsRef<str>) -> Result<Res>
    where
        Res: DeserializeOwned,
    {
        self.request::<(), _>(Method::DELETE, path, None).await
    }

    async fn get<Res>(&self, path: impl AsRef<str>) -> Result<Res>
    where
        Res: DeserializeOwned,
    {
        self.request::<(), _>(Method::GET, path, None).await
    }

    async fn post<Req, Res>(&self, path: impl AsRef<str>, data: Option<&Req>) -> Result<Res>
    where
        Req: ?Sized + Serialize,
        Res: DeserializeOwned,
    {
        self.request(Method::POST, path, data).await
    }

    async fn request<Req, Res>(
        &self,
        method: Method,
        path: impl AsRef<str>,
        data: Option<&Req>,
    ) -> Result<Res>
    where
        Req: ?Sized + Serialize,
        Res: DeserializeOwned,
    {
        let mut request = self.client.request(method, self.get_url(path));
        if let Some(data) = data {
            request = request.json(data);
        }
        if let Some(namespace) = &self.namespace {
            request = request.header(::ark_api::consts::HEADER_NAMESPACE, namespace);
        }

        let response = request.send().await?;
        match response.json().await? {
            SessionResult::Ok(data) => Ok(data),
            SessionResult::Err(error) => Err(anyhow!(error)),
        }
    }

    fn get_url(&self, path: impl AsRef<str>) -> Url {
        let path = path.as_ref();

        let mut url = self.host.clone();
        match url.path() {
            "/" => url.set_path(path),
            prefix => url.set_path(&format!("{prefix}{path}")),
        }
        url
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    pub name: String,
    pub namespace: String,
}
