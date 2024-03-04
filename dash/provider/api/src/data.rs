use std::iter::Sum;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Copy, Clone, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
pub struct Capacity {
    pub capacity: u128,
    pub usage: u128,
}

impl Capacity {
    pub const fn available(&self) -> u128 {
        let Self { capacity, usage } = *self;

        if usage <= capacity {
            capacity - usage
        } else {
            0
        }
    }

    pub fn limit_on(self, limit: u128) -> Self {
        let Self { capacity, usage } = self;
        Self {
            capacity: capacity.min(limit),
            usage,
        }
    }

    pub fn ratio(&self) -> f64 {
        let Self { capacity, usage } = *self;

        if capacity > 0 && usage <= capacity {
            usage as f64 / capacity as f64
        } else {
            1.0
        }
    }
}

impl Sum for Capacity {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), |a, b| Self {
            capacity: a.capacity + b.capacity,
            usage: a.usage + b.usage,
        })
    }
}
