use crate::df::DataFrame;

#[derive(Default)]
pub struct Context {
    pub(crate) edges: Option<DataFrame>,
    pub(crate) nodes: Option<DataFrame>,
    pub(crate) vm: crate::lazy::LazyVirtualMachine,
}

impl Context {}
