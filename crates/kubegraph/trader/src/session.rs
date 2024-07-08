use kubegraph_api::{frame::LazyFrame, trader::NetworkTraderContext};

#[derive(Clone)]
pub(crate) struct NetworkTraderSession {
    pub(crate) ctx: NetworkTraderContext<LazyFrame>,
}
