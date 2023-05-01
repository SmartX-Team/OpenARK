use anyhow::{anyhow, Result};
use dash_api::{function::FunctionCrd, model::ModelCrd};
use dash_provider_api::{FunctionChannel, SessionResult};
use reqwest::{Client, Method, Url};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

pub struct DashClient {
    client: Client,
    host: Url,
}

impl DashClient {
    pub async fn get_function(&self, name: &str) -> Result<FunctionCrd> {
        self.get(format!("/function/{name}/")).await
    }

    pub async fn get_function_list(&self) -> Result<Vec<String>> {
        self.get("/function/").await
    }

    pub async fn post_function(&self, name: &str, value: &Value) -> Result<FunctionChannel> {
        self.post(format!("/function/{name}"), Some(value)).await
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
        let mut url = self.host.clone();
        url.set_path(path.as_ref());
        url
    }
}
