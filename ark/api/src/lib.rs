pub mod package;

pub mod consts {
    pub const NAMESPACE: &str = "ark";
}

pub trait NamespaceAny {
    fn namespace_any(&self) -> String;
}

impl<T> NamespaceAny for T
where
    T: ::kube::ResourceExt,
{
    fn namespace_any(&self) -> String {
        self.namespace().unwrap_or_else(|| "default".into())
    }
}
