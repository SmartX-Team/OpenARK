mod page;

use anyhow::{bail, Error, Result};
use ark_core::signal::FunctionSignal;
use ark_core_k8s::data::Url;
use async_trait::async_trait;
use clap::Parser;
use futures::Stream;
use kubegraph_api::{
    component::NetworkComponent,
    market::{
        price::{PriceHistogram, PriceItem},
        product::ProductSpec,
        r#pub::PubSpec,
        sub::SubSpec,
        transaction::{TransactionSpec, TransactionTemplate},
        BaseModel, Page,
    },
};
use reqwest::Method;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{instrument, Level};

#[derive(Clone)]
pub struct MarketClient {
    args: MarketClientArgs,
    session: ::reqwest::Client,
}

#[async_trait]
impl NetworkComponent for MarketClient {
    type Args = MarketClientArgs;

    async fn try_new(args: <Self as NetworkComponent>::Args, _: &FunctionSignal) -> Result<Self> {
        Ok(Self {
            args,
            session: ::reqwest::ClientBuilder::new().build()?,
        })
    }
}

impl MarketClient {
    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get_product(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
    ) -> Result<Option<ProductSpec>> {
        let request = RequestWithoutPayload {
            method: Method::GET,
            rel_url: &format!("prod/{prod_id}"),
            page: None,
            payload: None,
        };
        self.execute(request).await
    }

    #[instrument(level = Level::INFO, skip(self, spec))]
    pub async fn find_product(&self, spec: &ProductSpec) -> Result<<ProductSpec as BaseModel>::Id> {
        let request = Request {
            method: Method::POST,
            rel_url: "prod",
            page: None,
            payload: Some(spec),
        };
        self.execute(request).await
    }

    pub fn list_product_ids(
        &self,
    ) -> impl '_ + Stream<Item = Result<<ProductSpec as BaseModel>::Id>> {
        let loader = move |page| self.list_product_ids_paged(page);
        let id_picker = move |id: &<ProductSpec as BaseModel>::Id| *id;
        self::page::create_stream(loader, id_picker)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list_product_ids_paged(
        &self,
        page: Page<usize>,
    ) -> Result<Vec<<ProductSpec as BaseModel>::Id>> {
        let request = RequestWithoutPayload {
            method: Method::GET,
            rel_url: "prod",
            page: Some(page),
            payload: None,
        };
        self.execute(request).await
    }

    #[instrument(level = Level::INFO, skip(self, spec))]
    pub async fn insert_product(&self, spec: &ProductSpec) -> Result<()> {
        let request = Request {
            method: Method::PUT,
            rel_url: "prod",
            page: None,
            payload: Some(spec),
        };
        self.execute(request).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_product(&self, prod_id: <ProductSpec as BaseModel>::Id) -> Result<()> {
        let request = RequestWithoutPayload {
            method: Method::DELETE,
            rel_url: &format!("prod/{prod_id}"),
            page: None,
            payload: None,
        };
        self.execute(request).await
    }
}

impl MarketClient {
    pub fn list_price_histogram(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
    ) -> impl '_ + Stream<Item = Result<PriceItem>> {
        let loader = move |page| self.list_price_histogram_paged(prod_id, page);
        let id_picker = move |item: &PriceItem| item.id;
        self::page::create_stream(loader, id_picker)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list_price_histogram_paged(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        page: Page<usize>,
    ) -> Result<PriceHistogram> {
        let request = RequestWithoutPayload {
            method: Method::GET,
            rel_url: &format!("prod/{prod_id}/price"),
            page: Some(page),
            payload: None,
        };
        self.execute(request).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn trade(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        template: &TransactionTemplate,
    ) -> Result<<TransactionSpec as BaseModel>::Id> {
        let request = Request {
            method: Method::POST,
            rel_url: &format!("prod/{prod_id}/trade"),
            page: None,
            payload: Some(template),
        };
        self.execute(request).await
    }
}

impl MarketClient {
    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get_pub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        pub_id: <PubSpec as BaseModel>::Id,
    ) -> Result<Option<PubSpec>> {
        let request = RequestWithoutPayload {
            method: Method::GET,
            rel_url: &format!("prod/{prod_id}/pub/{pub_id}"),
            page: None,
            payload: None,
        };
        self.execute(request).await
    }

    pub fn list_pub_ids(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
    ) -> impl '_ + Stream<Item = Result<<PubSpec as BaseModel>::Id>> {
        let loader = move |page| self.list_pub_ids_paged(prod_id, page);
        let id_picker = move |id: &<PubSpec as BaseModel>::Id| *id;
        self::page::create_stream(loader, id_picker)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list_pub_ids_paged(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        page: Page<usize>,
    ) -> Result<Vec<<PubSpec as BaseModel>::Id>> {
        let request = RequestWithoutPayload {
            method: Method::GET,
            rel_url: &format!("prod/{prod_id}/pub"),
            page: Some(page),
            payload: None,
        };
        self.execute(request).await
    }

    #[instrument(level = Level::INFO, skip(self, spec))]
    pub async fn insert_pub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        spec: &PubSpec,
    ) -> Result<()> {
        let request = Request {
            method: Method::PUT,
            rel_url: &format!("prod/{prod_id}/pub"),
            page: None,
            payload: Some(spec),
        };
        self.execute(request).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_pub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        pub_id: <PubSpec as BaseModel>::Id,
    ) -> Result<()> {
        let request = RequestWithoutPayload {
            method: Method::DELETE,
            rel_url: &format!("prod/{prod_id}/pub/{pub_id}"),
            page: None,
            payload: None,
        };
        self.execute(request).await
    }
}

impl MarketClient {
    #[instrument(level = Level::INFO, skip(self))]
    pub async fn get_sub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        sub_id: <SubSpec as BaseModel>::Id,
    ) -> Result<Option<SubSpec>> {
        let request = RequestWithoutPayload {
            method: Method::GET,
            rel_url: &format!("prod/{prod_id}/sub/{sub_id}"),
            page: None,
            payload: None,
        };
        self.execute(request).await
    }

    pub fn list_sub_ids(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
    ) -> impl '_ + Stream<Item = Result<<SubSpec as BaseModel>::Id>> {
        let loader = move |page| self.list_sub_ids_paged(prod_id, page);
        let id_picker = move |id: &<SubSpec as BaseModel>::Id| *id;
        self::page::create_stream(loader, id_picker)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list_sub_ids_paged(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        page: Page<usize>,
    ) -> Result<Vec<<SubSpec as BaseModel>::Id>> {
        let request = RequestWithoutPayload {
            method: Method::GET,
            rel_url: &format!("prod/{prod_id}/sub"),
            page: Some(page),
            payload: None,
        };
        self.execute(request).await
    }

    #[instrument(level = Level::INFO, skip(self, spec))]
    pub async fn insert_sub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        spec: &SubSpec,
    ) -> Result<()> {
        let request = Request {
            method: Method::PUT,
            rel_url: &format!("prod/{prod_id}/sub"),
            page: None,
            payload: Some(spec),
        };
        self.execute(request).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn remove_sub(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        sub_id: <SubSpec as BaseModel>::Id,
    ) -> Result<()> {
        let request = RequestWithoutPayload {
            method: Method::DELETE,
            rel_url: &format!("prod/{prod_id}/sub/{sub_id}"),
            page: None,
            payload: None,
        };
        self.execute(request).await
    }
}

impl MarketClient {
    #[instrument(level = Level::INFO, skip(self, request))]
    async fn execute<T, R>(&self, request: Request<'_, T>) -> Result<R>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let Request {
            method,
            rel_url,
            page,
            payload,
        } = request;

        let url = self.args.endpoint.join(rel_url)?;
        let mut request = match method.as_str() {
            "GET" => self.session.get(url),
            "DELETE" => self.session.delete(url),
            "POST" => self.session.post(url),
            "PUT" => self.session.put(url),
            _ => bail!("unsupported method: {method}"),
        };
        if let Some(page) = page {
            request = request.query(&page);
        }
        if let Some(payload) = payload {
            request = request.json(&payload);
        }

        request
            .send()
            .await?
            .json::<::ark_core::result::Result<R>>()
            .await
            .map_err(Into::into)
            .and_then(|result| match result {
                ::ark_core::result::Result::Ok(data) => Ok(data),
                ::ark_core::result::Result::Err(error) => Err(Error::msg(error)),
            })
    }
}

type RequestWithoutPayload<'a> = Request<'a, ()>;

struct Request<'a, T> {
    method: Method,
    rel_url: &'a str,
    page: Option<Page<usize>>,
    payload: Option<&'a T>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, Parser)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct MarketClientArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_MARKET_CLIENT_ENDPOINT",
        value_name = "URL",
        value_enum,
        default_value = MarketClientArgs::default_endpoint_str(),
    )]
    #[serde(default = "MarketClientArgs::default_endpoint")]
    pub endpoint: Url,
}

impl Default for MarketClientArgs {
    fn default() -> Self {
        Self {
            endpoint: Self::default_endpoint(),
        }
    }
}

impl MarketClientArgs {
    const fn default_endpoint_str() -> &'static str {
        "http://market.kubegraph.svc"
    }

    fn default_endpoint() -> Url {
        Self::default_endpoint_str().parse().unwrap()
    }
}
