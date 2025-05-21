/*
  logger.rs
*/

use std::fs::create_dir_all;

use tracing::Level;
use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_logger() {
    let log_dir = "logs";
    create_dir_all(log_dir).expect("Failed to create logs directory.");

    let file_appender = rolling::daily(log_dir, "rclaim.log");
    let file_layer = fmt::layer()
        .json()
        .with_writer(file_appender)
        .with_ansi(false);

    let console_layer = fmt::layer()
        .pretty()
        .with_target(true)
        .with_line_number(true)
        .with_file(true);

    let env_filter = EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();

    tracing_subscriber::registry()
        .with(file_layer)
        .with(console_layer)
        .with(env_filter)
        .init();
}
