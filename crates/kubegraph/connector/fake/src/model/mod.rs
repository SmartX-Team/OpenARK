mod constant;
mod name;
mod normal;

use anyhow::{anyhow, Error, Result};
use kubegraph_api::{
    connector::fake::{
        NetworkConnectorFakeData, NetworkConnectorFakeDataFrame, NetworkConnectorFakeDataModel,
    },
    frame::LazyFrame,
    graph::GraphScope,
};
use polars::{error::PolarsError, frame::DataFrame, series::Series};

pub trait DataGenerator<'a> {
    type Args;
    type Error;
    type Output;

    fn generate(
        self,
        args: <Self as DataGenerator<'a>>::Args,
    ) -> Result<<Self as DataGenerator<'a>>::Output, <Self as DataGenerator<'a>>::Error>;
}

impl<'a> DataGenerator<'a> for Option<NetworkConnectorFakeData> {
    type Args = &'a GraphScope;
    type Error = Error;
    type Output = LazyFrame;

    fn generate(
        self,
        scope: <Self as DataGenerator<'a>>::Args,
    ) -> Result<<Self as DataGenerator<'a>>::Output, <Self as DataGenerator<'a>>::Error> {
        match self {
            Some(data) => data.generate(scope).map(Into::into),
            None => Ok(LazyFrame::Empty),
        }
    }
}

impl<'a> DataGenerator<'a> for NetworkConnectorFakeData {
    type Args = &'a GraphScope;
    type Error = Error;
    type Output = DataFrame;

    fn generate(
        self,
        scope: <Self as DataGenerator<'a>>::Args,
    ) -> Result<<Self as DataGenerator<'a>>::Output, <Self as DataGenerator<'a>>::Error> {
        let Self { count, frame } = self;
        frame.generate((scope, count))
    }
}

impl<'a> DataGenerator<'a> for NetworkConnectorFakeDataFrame {
    type Args = (&'a GraphScope, usize);
    type Error = Error;
    type Output = DataFrame;

    fn generate(
        self,
        (scope, count): <Self as DataGenerator<'a>>::Args,
    ) -> Result<<Self as DataGenerator<'a>>::Output, <Self as DataGenerator<'a>>::Error> {
        let Self { map } = self;
        let columns = map
            .into_iter()
            .map(|(key, model)| {
                model
                    .generate((scope, count))
                    .map(|data| data.with_name(&key))
                    .map_err(|error| anyhow!("on {key}: {error}"))
            })
            .collect::<Result<Vec<_>>>()?;
        DataFrame::new(columns).map_err(Into::into)
    }
}

impl<'a> DataGenerator<'a> for NetworkConnectorFakeDataModel {
    type Args = (&'a GraphScope, usize);
    type Error = PolarsError;
    type Output = Series;

    fn generate(
        self,
        (scope, count): <Self as DataGenerator<'a>>::Args,
    ) -> Result<<Self as DataGenerator<'a>>::Output, <Self as DataGenerator<'a>>::Error> {
        match self {
            Self::Constant(model) => model.generate(count),
            Self::Name(model) => model.generate((scope, count)),
            Self::Normal(model) => model.generate(count),
        }
    }
}
