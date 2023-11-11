use std::iter::Sum;

use byte_unit::Byte;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Capacity {
    pub capacity: Byte,
    pub usage: Byte,
}

impl Capacity {
    pub const fn available(&self) -> Byte {
        let capacity = self.capacity.get_bytes();
        let usage = self.usage.get_bytes();

        if usage <= capacity {
            Byte::from_bytes(usage - capacity)
        } else {
            Byte::from_bytes(0)
        }
    }

    pub fn limit_on(self, limit: Byte) -> Self {
        let Self { capacity, usage } = self;
        Self {
            capacity: capacity.min(limit),
            usage,
        }
    }

    pub fn ratio(&self) -> f64 {
        let capacity = self.capacity.get_bytes();
        let usage = self.usage.get_bytes();

        if capacity > 0 {
            if usage <= capacity {
                usage as f64 / capacity as f64
            } else {
                1.0
            }
        } else {
            0.0
        }
    }
}

impl Sum for Capacity {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), |a, b| Self {
            capacity: Byte::from_bytes(a.capacity.get_bytes() + b.capacity.get_bytes()),
            usage: Byte::from_bytes(a.usage.get_bytes() + b.usage.get_bytes()),
        })
    }
}
