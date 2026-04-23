use tracing_subscriber::{fmt, EnvFilter};

pub fn init(service_name: &str, rust_log: Option<&str>) {
    let filter = rust_log
        .map(str::to_owned)
        .or_else(|| std::env::var("RUST_LOG").ok())
        .unwrap_or_else(|| format!("info,{service_name}=debug"));

    let subscriber = fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_target(true)
        .with_thread_ids(true)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent() {
        init("test_service", Some("debug"));
        init("test_service", Some("debug"));
    }
}
