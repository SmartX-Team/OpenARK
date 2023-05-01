pub fn init_once() {
    // set default log level
    const KEY: &str = "RUST_LOG";
    if ::std::env::var_os(KEY).is_none() {
        ::std::env::set_var(KEY, "INFO");
    }

    ::pretty_env_logger::try_init().ok();
}
