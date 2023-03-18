pub mod client;
pub mod input;
mod source;

pub mod name {
    pub const RE: &str = r"^/([a-z_-][a-z0-9_-]*[a-z0-9]?/)*$";
    pub const RE_CHILD: &str = r"^[a-z_-][a-z0-9_-]*[a-z0-9]?$";
    pub const RE_SET: &str = r"^/([a-z_-][a-z0-9_-]*[a-z0-9]?/)+$";
}

pub(crate) const NAME: &str = "dash-actor";
