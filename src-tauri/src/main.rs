#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tracing_subscriber::EnvFilter;

fn main() {
    // Default to info level, can be overridden by RUST_LOG env var
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,ccg_gateway=debug,ccg_gateway_lib=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    ccg_gateway_lib::run();
}
