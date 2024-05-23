use kubegraph_api::connector::fake::NetworkConnectorFakeDataValueType;
use polars::{error::PolarsError, series::Series};
use rand::Rng;
use rand_distr::Distribution;

pub(super) struct GenericDistModel<D, R> {
    pub(super) count: usize,
    pub(super) dist: D,
    pub(super) rng: R,
    pub(super) value_type: NetworkConnectorFakeDataValueType,
}

impl<'a, D, R> super::DataGenerator<'a> for GenericDistModel<D, R>
where
    D: Distribution<f64>,
    R: Rng,
{
    type Args = ();
    type Error = PolarsError;
    type Output = Series;

    fn generate(
        self,
        (): <Self as super::DataGenerator<'a>>::Args,
    ) -> Result<<Self as super::DataGenerator<'a>>::Output, <Self as super::DataGenerator<'a>>::Error>
    {
        let Self {
            count,
            dist,
            rng,
            value_type,
        } = self;

        Series::from_iter(rng.sample_iter::<f64, _>(dist).take(count)).cast(&value_type.into())
    }
}
