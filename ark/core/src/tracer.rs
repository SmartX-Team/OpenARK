use std::ffi::OsStr;

pub fn init_once() {
    // set default tracing level
    if ::std::env::var_os(KEY).is_none() {
        ::std::env::set_var(KEY, "INFO");
    }

    ::tracing_subscriber::fmt::try_init().ok();
}

pub fn init_once_with(level: impl AsRef<OsStr>) {
    // set custom tracing level
    ::std::env::set_var(KEY, level);

    ::tracing_subscriber::fmt::try_init().ok();
}

const KEY: &str = "RUST_LOG";
