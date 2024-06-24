use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::problem::ProblemSpec;

use super::{r#pub::PubSpec, sub::SubSpec};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct PriceHistogram<Pub = PubSpec, Sub = SubSpec>
where
    Pub: super::BaseModel,
    Sub: super::BaseModel,
{
    pub r#pub: BTreeMap<<Pub as super::BaseModel>::Id, <Pub as super::BaseModel>::Cost>,
    pub sub: BTreeMap<<Sub as super::BaseModel>::Id, <Sub as super::BaseModel>::Cost>,
}

impl<Pub, Sub> Default for PriceHistogram<Pub, Sub>
where
    Pub: super::BaseModel,
    Sub: super::BaseModel,
{
    fn default() -> Self {
        Self {
            r#pub: BTreeMap::default(),
            sub: BTreeMap::default(),
        }
    }
}

impl<Pub, Sub> super::BaseModel for PriceHistogram<Pub, Sub>
where
    Pub: super::BaseModel,
    Sub: super::BaseModel,
{
    const KEY: &'static str = "price";

    type Id = <ProductSpec as super::BaseModel>::Id;
    type Cost = <ProductSpec as super::BaseModel>::Cost;
    type Count = <ProductSpec as super::BaseModel>::Count;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductSpec {
    pub problem: ProblemSpec,
}

impl super::BaseModel for ProductSpec {
    const KEY: &'static str = "prod";

    type Id = Uuid;
    type Cost = u64;
    type Count = u64;
}
