use tracing_subscriber::EnvFilter;

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,cab_gateway=debug,cab_api=debug")),
        )
        .init();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    {
        let is_service = std::env::args().any(|a| a == "--service");
        if is_service {
            // SCM starts us before a console exists; init tracing inside the service path too.
            init_tracing();
            return cab_srv::windows_service::run_as_service();
        }
    }

    init_tracing();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(cab_srv::run_server())
}
