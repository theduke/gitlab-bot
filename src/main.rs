#![feature(proc_macro, conservative_impl_trait, generators)]

extern crate chrono;
#[macro_use]
extern crate failure;
extern crate futures_await as futures;
extern crate regex;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate slog;
extern crate sloggers;
extern crate tokio_core;
extern crate toml;

// Needed for static musl build.
// See https://github.com/emk/rust-musl-builder
extern crate openssl_probe;

mod client;
mod bot;

use tokio_core::reactor::Core;



fn main() {
    // Needed for static musl build.
    // See https://github.com/emk/rust-musl-builder
    openssl_probe::init_ssl_cert_env_vars();

    let mut core = Core::new().unwrap();

    let bot = bot::Bot::from_env(core.handle()).unwrap();
    core.run(bot.run()).unwrap();
}
