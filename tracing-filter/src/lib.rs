/// The default quiet filters in addition to `RUST_LOG`.
pub const RUST_LOG_SILENCE_DEFAULTS: &str =
    "actix_web=info,actix_server=info,async_nats=info,h2=info,hyper=info,tower_buffer=info,tower=info,rustls=info,reqwest=info,tokio_util=info,tokio_tungstenite=info,tungstenite=info,async_io=info,polling=info,tonic=info,want=info,mio=info";

/// Mix the `RUST_LOG` `EnvFilter` with `RUST_LOG_SILENCE`.
/// This is useful when we want to bulk-silence certain crates by default.
pub fn rust_log_add_quiet_defaults(
    current: tracing_subscriber::EnvFilter,
) -> tracing_subscriber::EnvFilter {
    let rust_log_silence = std::env::var("RUST_LOG_SILENCE");
    let silence = match &rust_log_silence {
        Ok(quiets) => quiets.as_str(),
        Err(_) => RUST_LOG_SILENCE_DEFAULTS,
    };

    tracing_subscriber::EnvFilter::try_new(match silence.is_empty() {
        true => current.to_string(),
        false => format!("{current},{silence}"),
    })
    .unwrap()
}

/// Get the `tracing_subscriber::EnvFilter`, taken by mixing RUST_LOG
/// and RUST_LOG_SILENCE which silences unwanted traces.
/// If no default is set from env, use the provided filter.
pub fn rust_log_filter_ext(level: &str) -> tracing_subscriber::EnvFilter {
    rust_log_add_quiet_defaults(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
    )
}

/// Get the `tracing_subscriber::EnvFilter`, taken by mixing RUST_LOG
/// and RUST_LOG_SILENCE which silences unwanted traces.
pub fn rust_log_filter() -> tracing_subscriber::EnvFilter {
    rust_log_filter_ext("info")
}
