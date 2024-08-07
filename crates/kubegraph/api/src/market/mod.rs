pub mod price;
pub mod product;
pub mod r#pub;
pub mod sub;
pub mod transaction;

use std::{fmt, hash::Hash};

use num_traits::Num;
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

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "T: Default + Serialize + DeserializeOwned + Num")]
#[serde(rename_all = "camelCase")]
pub struct Page<T = u64> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Uuid>,
    #[serde(default = "Page::default_limit")]
    pub limit: T,
}

impl Default for Page<u64> {
    fn default() -> Self {
        Self {
            start: None,
            limit: Self::default_limit_u64(),
        }
    }
}

impl Default for Page<usize> {
    fn default() -> Self {
        Self {
            start: None,
            limit: Self::default_limit_usize(),
        }
    }
}

impl<T> Page<T>
where
    T: Default + Num,
{
    fn default_limit() -> T {
        <T as Num>::from_str_radix("20", 10).unwrap_or_default()
    }
}

impl Page<u64> {
    const fn default_limit_u64() -> u64 {
        20
    }
}

impl Page<usize> {
    const fn default_limit_usize() -> usize {
        20
    }
}
