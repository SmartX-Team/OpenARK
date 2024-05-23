use kubegraph_api::connector::fake::model::ConstantModel;
use polars::{error::PolarsError, series::Series};

impl<'a> super::DataGenerator<'a> for ConstantModel {
    type Args = usize;
    type Error = PolarsError;
    type Output = Series;

    fn generate(
        self,
        count: <Self as super::DataGenerator<'a>>::Args,
    ) -> Result<<Self as super::DataGenerator<'a>>::Output, <Self as super::DataGenerator<'a>>::Error>
    {
        let Self { value, value_type } = self;
        Series::from_iter((0..count).map(|_| value)).cast(&value_type.into())
    }
}
