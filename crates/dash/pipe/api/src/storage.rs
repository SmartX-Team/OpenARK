use ark_core_k8s::data::Url;
use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct StorageS3Args {
    #[arg(long, env = "AWS_ACCESS_KEY_ID", value_name = "VALUE")]
    pub access_key: String,

    #[arg(
        long,
        env = "AWS_REGION",
        value_name = "REGION",
        default_value_t = Self::default_region().into(),
    )]
    pub region: String,

    #[arg(long, env = "AWS_ENDPOINT_URL", value_name = "URL")]
    pub s3_endpoint: Url,

    #[arg(long, env = "AWS_SECRET_ACCESS_KEY", value_name = "VALUE")]
    pub secret_key: String,
}

impl StorageS3Args {
    pub const fn default_region() -> &'static str {
        "us-east-1"
    }
}
