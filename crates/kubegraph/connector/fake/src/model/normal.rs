use kubegraph_api::connector::fake::model::NormalModel;
use polars::{error::PolarsError, series::Series};
use rand::{rngs::SmallRng, thread_rng, SeedableRng};
use rand_distr::Normal;

use super::dist::GenericDistModel;

impl<'a> super::DataGenerator<'a> for NormalModel {
    type Args = usize;
    type Error = PolarsError;
    type Output = Series;

    fn generate(
        self,
        count: <Self as super::DataGenerator<'a>>::Args,
    ) -> Result<<Self as super::DataGenerator<'a>>::Output, <Self as super::DataGenerator<'a>>::Error>
    {
        let Self {
            mean,
            seed,
            std: std_dev,
            value_type,
        } = self;

        let dist = Normal::new(mean, std_dev)
            .map_err(|error| PolarsError::ComputeError(error.to_string().into()))?;

        match seed {
            Some(seed) => GenericDistModel {
                count,
                dist,
                rng: SmallRng::seed_from_u64(seed),
                value_type,
            }
            .generate(()),
            None => GenericDistModel {
                count,
                dist,
                rng: thread_rng(),
                value_type,
            }
            .generate(()),
        }
    }
}
