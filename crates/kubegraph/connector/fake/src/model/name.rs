use kubegraph_api::{connector::fake::model::NameModel, graph::GraphScope};
use polars::{error::PolarsError, series::Series};

impl<'a> super::DataGenerator<'a> for NameModel {
    type Args = (&'a GraphScope, usize);
    type Error = PolarsError;
    type Output = Series;

    fn generate(
        self,
        (scope, count): <Self as super::DataGenerator<'a>>::Args,
    ) -> Result<<Self as super::DataGenerator<'a>>::Output, <Self as super::DataGenerator<'a>>::Error>
    {
        let GraphScope { name, .. } = scope;
        let Self { prefix } = self;

        let prefix = prefix.unwrap_or_else(|| format!("{name}-"));

        Ok(Series::from_iter(
            (0..count).map(|index| format!("{prefix}{index}")),
        ))
    }
}
