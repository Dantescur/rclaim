/*
  logger.rs
*/

use std::{env, fs::create_dir_all};

use tracing::Level;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_logger() {
    let log_dir = "logs";
    create_dir_all(log_dir).expect("Failed to create logs directory.");

    let console_layer = fmt::layer()
        .pretty()
        .with_target(true)
        .with_line_number(true)
        .with_file(true);

    let env_filter = EnvFilter::builder()
        .with_default_directive(
            env::var("RUST_LOG")
                .unwrap_or("info".to_string())
                .parse()
                .unwrap_or(Level::INFO.into()),
        )
        .from_env_lossy();

    tracing_subscriber::registry()
        .with(console_layer)
        .with(env_filter)
        .init();
}
