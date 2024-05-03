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
        let capacity = self.capacity.as_u128();
        let usage = self.usage.as_u128();

        if usage <= capacity {
            match Byte::from_u128(capacity - usage) {
                Some(bytes) => bytes,
                None => Byte::MIN,
            }
        } else {
            Byte::MIN
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
        let capacity = self.capacity.as_u128();
        let usage = self.usage.as_u128();

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
            capacity: Byte::from_u128(a.capacity.as_u128() + b.capacity.as_u128())
                .unwrap_or(Byte::MAX),
            usage: Byte::from_u128(a.usage.as_u128() + b.usage.as_u128()).unwrap_or(Byte::MAX),
        })
    }
}
