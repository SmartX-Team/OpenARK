use kubegraph_api::frame::LazyFrame;

#[derive(Default)]
pub struct Context {
    pub(crate) edges: Option<LazyFrame>,
    pub(crate) nodes: Option<LazyFrame>,
    pub(crate) vm: crate::lazy::LazyVirtualMachine,
}

impl Context {}
