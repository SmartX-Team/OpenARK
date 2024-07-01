pub mod price;
pub mod product;
pub mod r#pub;
pub mod sub;

use std::{fmt, hash::Hash};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

pub trait BaseModel
where
    Self: Serialize + DeserializeOwned,
{
    type Id: Copy + fmt::Debug + fmt::Display + Eq + Ord + Hash + Serialize + DeserializeOwned;

    type Cost: Copy + fmt::Debug + Eq + Ord + Serialize + DeserializeOwned;

    type Count: Copy + fmt::Debug + Eq + Ord;
}

pub trait BaseModelItem
where
    Self: BaseModel,
{
    fn cost(&self) -> <Self as BaseModel>::Cost;
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Uuid>,
    #[serde(default = "Page::default_limit")]
    pub limit: u64,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            start: None,
            limit: Self::default_limit(),
        }
    }
}

impl Page {
    const fn default_limit() -> u64 {
        20
    }
}
