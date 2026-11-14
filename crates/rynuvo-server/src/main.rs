use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use clap::{Parser, Subcommand};
use rynuvo_server::{
    app::{AppState, build_router},
    config::Config,
    db,
    disk::DiskState,
    ws::Hub,
};

#[derive(Parser)]
#[command(name = "rynuvo-server", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the server.
    Serve {
        #[arg(long, default_value = "/etc/rynuvo/config.toml")]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Serve { config } => serve(Config::load(&config)?).await,
    }
}

async fn open(cfg: &Config) -> anyhow::Result<sqlx::SqlitePool> {
    std::fs::create_dir_all(cfg.media_dir())?;
    std::fs::create_dir_all(cfg.downloads_dir())?;
    db::open_pool(&format!("sqlite://{}", cfg.db_path().display())).await
}

async fn serve(cfg: Config) -> anyhow::Result<()> {
    let pool = open(&cfg).await?;
    let state = AppState {
        db: pool,
        disk: DiskState::new(cfg.disk_block_pct),
        cfg: Arc::new(cfg.clone()),
        hub: Hub::default(),
    };
    let listener = tokio::net::TcpListener::bind(cfg.bind).await?;
    tracing::info!("rynuvo-server listening on {}", cfg.bind);
    axum::serve(
        listener,
        build_router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
