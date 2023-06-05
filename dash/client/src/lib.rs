use std::error::Error;

use anyhow::{anyhow, Result};
use ark_core::result::Result as SessionResult;
use dash_api::{function::FunctionCrd, job::DashJobCrd, model::ModelCrd};
use reqwest::{Client, Method, Url};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

pub struct DashClient {
    client: Client,
    host: Url,
}

impl DashClient {
    pub fn new(client: Client, host: Url) -> Self {
        Self { client, host }
    }

    pub fn with_host<Host>(host: Host) -> Result<Self>
    where
        Host: TryInto<Url>,
        <Host as TryInto<Url>>::Error: 'static + Send + Sync + Error,
    {
        host.try_into()
            .map(|host| Self::new(Default::default(), host))
            .map_err(Into::into)
    }
}

impl DashClient {
    pub async fn get_function(&self, name: &str) -> Result<FunctionCrd> {
        self.get(format!("/function/{name}/")).await
    }

    pub async fn get_function_list(&self) -> Result<Vec<String>> {
        self.get("/function/").await
    }
}

impl DashClient {
    pub async fn post_job(&self, function_name: &str, value: &Value) -> Result<DashJobCrd> {
        self.post(format!("/function/{function_name}/job/"), Some(value))
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

    pub async fn get_model_list(&self) -> Result<Vec<String>> {
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
    async fn get<Res>(&self, path: impl AsRef<str>) -> Result<Res>
    where
        Res: DeserializeOwned,
    {
        self.request::<(), _>(Method::GET, path, None).await
    }

    async fn post<Req, Res>(&self, path: impl AsRef<str>, data: Option<&Req>) -> Result<Res>
    where
        Req: Serialize,
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
        Req: Serialize,
        Res: DeserializeOwned,
    {
        let mut request = self.client.request(method, self.get_url(path));
        if let Some(data) = data {
            request = request.json(data);
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
