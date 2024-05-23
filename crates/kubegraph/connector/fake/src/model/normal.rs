use kubegraph_api::connector::fake::model::NormalModel;
use polars::{error::PolarsError, series::Series};
use rand::{distributions::Standard, rngs::StdRng, Rng, SeedableRng};

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
            std,
            value_type,
        } = self;
        Series::from_iter(
            StdRng::from_entropy()
                .sample_iter::<f64, _>(Standard)
                .take(count),
        )
        .cast(&value_type.into())
    }
}
