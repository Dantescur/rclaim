use std::env;
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

const IS_PRETTY: bool = cfg!(debug_assertions);

pub fn init_logger() {
    let console_layer: Box<dyn Layer<_> + Send + Sync> = if IS_PRETTY {
        Box::new(
            fmt::layer()
                .pretty()
                .with_target(true)
                .with_line_number(true)
                .with_file(true),
        )
    } else {
        Box::new(
            fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .flatten_event(true)
                .with_target(true)
                .with_level(true),
        )
    };

    let env_filter = match env::var("RUST_LOG") {
        Ok(val) => EnvFilter::try_new(&val).unwrap_or_else(|err| {
            eprintln!("⚠️ Invalid RUST_LOG '{}': {}", val, err);
            EnvFilter::new("info")
        }),
        Err(_) => EnvFilter::new("info"),
    };

    tracing_subscriber::registry()
        .with(console_layer)
        .with(env_filter)
        .init();
}
