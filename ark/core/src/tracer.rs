pub fn init_once() {
    // set default tracing level
    const KEY: &str = "RUST_LOG";
    if ::std::env::var_os(KEY).is_none() {
        ::std::env::set_var(KEY, "INFO");
    }

    ::tracing_subscriber::fmt::try_init().ok();
}
