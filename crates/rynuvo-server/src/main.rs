use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use clap::{Parser, Subcommand};
use rynuvo_server::{
    app::{AppState, build_router},
    config::Config,
    db,
    disk::DiskState,
    routes::identity,
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
    /// Print a fresh one-time owner code (claims super-admin on join).
    OwnerCode {
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
        Cmd::OwnerCode { config } => {
            let cfg = Config::load(&config)?;
            let pool = open(&cfg).await?;
            let code = identity::new_owner_code(&pool).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
            println!("{code}");
            Ok(())
        }
    }
}

async fn open(cfg: &Config) -> anyhow::Result<sqlx::SqlitePool> {
    std::fs::create_dir_all(cfg.media_dir())?;
    std::fs::create_dir_all(cfg.downloads_dir())?;
    db::open_pool(&format!("sqlite://{}", cfg.db_path().display())).await
}

async fn serve(cfg: Config) -> anyhow::Result<()> {
    let pool = open(&cfg).await?;
    if let Some(code) = identity::ensure_owner_code(&pool).await.map_err(|e| anyhow::anyhow!("{e:?}"))? {
        tracing::warn!("owner code (join with this to become super-admin): {code}");
    }
    let hub = Hub::default();
    let voice = rynuvo_server::voice::VoiceRooms::default();
    let relay = rynuvo_server::voice::relay::Relay::new(voice.clone(), hub.clone());
    let state = AppState {
        db: pool,
        disk: DiskState::new(cfg.disk_warn_pct, cfg.disk_block_pct),
        cfg: Arc::new(cfg.clone()),
        hub,
        voice: voice.clone(),
        relay: relay.clone(),
        music: Default::default(),
        push: match &cfg.fcm_service_account {
            Some(f) => rynuvo_server::push::Push::fcm(f).inspect(|_| tracing::info!("push notifications on")).unwrap_or_else(|e| {
                tracing::error!("push notifications off, the Firebase key didn't load: {e:#}");
                Default::default()
            }),
            None => Default::default(),
        },
    };
    relay.quota.set_limit(cfg.quota_gb.saturating_mul(1_000_000_000));
    rynuvo_server::quota::spawn(state.db.clone(), relay.quota.clone(), state.hub.clone()).await;
    rynuvo_server::music::wire::spawn(state.clone());
    rynuvo_server::backup::spawn(state.db.clone(), cfg.backups_dir(), cfg.backups_keep);
    rynuvo_server::stun::spawn(cfg.stun_ports, Some(relay))?;
    rynuvo_server::voice::relay::Relay::spawn_drop_logger();
    rynuvo_server::disk::spawn_watcher(cfg.data_dir.clone(), state.disk.clone(), state.hub.clone());
    let listener = tokio::net::TcpListener::bind(cfg.bind).await?;
    tracing::info!("rynuvo-server listening on {}", cfg.bind);
    axum::serve(
        listener,
        build_router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
