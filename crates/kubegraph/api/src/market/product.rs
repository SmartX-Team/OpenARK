use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::problem::ProblemSpec;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductSpec {
    pub problem: ProblemSpec,
}

impl super::BaseModel for ProductSpec {
    type Id = Uuid;
    type Cost = i64;
    type Count = u64;
}
