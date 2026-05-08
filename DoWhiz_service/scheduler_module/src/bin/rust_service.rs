use scheduler_module::service::{run_server, ServiceConfig};
use std::env;
use tokio::sync::oneshot;
use tracing::info;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn parse_args() -> Result<(Option<String>, Option<u16>), String> {
    let mut host = None;
    let mut port = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => {
                host = args.next();
            }
            "--port" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --port".to_string())?;
                let parsed = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid port: {}", value))?;
                port = Some(parsed);
            }
            "--help" | "-h" => {
                return Err(help_text());
            }
            _ => {
                return Err(format!("unknown argument: {}", arg));
            }
        }
    }
    Ok((host, port))
}

fn help_text() -> String {
    [
        "DoWhiz worker service",
        "",
        "Usage:",
        "  cargo run -p scheduler_module --bin rust_service -- --host 0.0.0.0 --port 9001",
        "",
        "Options:",
        "  --host   Bind host (default: env RUST_SERVICE_HOST or 0.0.0.0)",
        "  --port   Bind port (default: env RUST_SERVICE_PORT or 9001)",
    ]
    .join("\n")
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate()).expect("register SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    tracing_subscriber::fmt().with_target(false).init();

    let (host_override, port_override) = match parse_args() {
        Ok(values) => values,
        Err(msg) => {
            eprintln!("{}", msg);
            return Ok(());
        }
    };

    let mut config = ServiceConfig::from_env()?;
    if let Some(host) = host_override {
        config.host = host;
    }
    if let Some(port) = port_override {
        config.port = port;
    }

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        let _ = shutdown_tx.send(());
    });

    info!(
        "starting service employee_id={} runner={} users_root={} users_db_path={} task_index_path={}",
        config.employee_id,
        config.employee_profile.runner,
        config.users_root.display(),
        config.users_db_path.display(),
        config.task_index_path.display()
    );

    run_server(config, async {
        let _ = shutdown_rx.await;
    })
    .await?;

    Ok(())
}
