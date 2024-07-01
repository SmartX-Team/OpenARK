use std::future::Future;

use anyhow::Result;
use async_stream::try_stream;
use futures::Stream;
use kubegraph_api::market::{product::ProductSpec, BaseModel, Page};

pub(crate) fn create_stream<F, Fut, T>(
    loader: F,
    id_picker: impl Copy + Fn(&T) -> <ProductSpec as BaseModel>::Id,
) -> impl Stream<Item = Result<T>>
where
    F: Fn(Page<usize>) -> Fut,
    Fut: Future<Output = Result<Vec<T>>>,
{
    try_stream! {
        let mut page = Page::default();

        loop {
            let items = loader(page).await?;
            page.start = items.last().map(id_picker);
            let len = items.len();

            for item in items {
                yield item;
            }

            if len != page.limit {
                break;
            }
        }
    }
}
