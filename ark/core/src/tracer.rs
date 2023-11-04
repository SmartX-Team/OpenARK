use std::{env, ffi::OsStr};

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

pub fn init_once_with_level_int(level: u8) {
    // You can see how many times a particular flag or argument occurred
    // Note, only flags can have multiple occurrences
    let debug_level = match level {
        0 => "WARN",
        1 => "INFO",
        2 => "DEBUG",
        3 => "TRACE",
        level => panic!("too high debug level: {level}"),
    };
    env::set_var("RUST_LOG", debug_level);
    init_once();
}

const KEY: &str = "RUST_LOG";
