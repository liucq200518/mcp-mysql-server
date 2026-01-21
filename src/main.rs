use crate::config::AppConfig;
use crate::db::DbManager;
use clap::Parser;
use notify::{Event, RecursiveMode, Watcher};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

mod config;
mod db;
mod mcp;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the configuration file
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,

    /// Port to listen on for HTTP/SSE (if not specified, uses stdio)
    #[arg(short, long, value_name = "PORT")]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging to stderr to avoid interfering with MCP stdio transport
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let args = Args::parse();
    info!("Starting MCP MySQL Server...");
    info!("Loading config from: {:?}", args.config);

    let config = AppConfig::load_from_file(&args.config)?;

    info!("Initializing database connections...");
    let db_manager = DbManager::new(&config).await?;

    // Start config file watcher
    let (tx, mut rx) = mpsc::unbounded_channel();
    let config_path = args.config.clone();
    let db_manager_reloader = db_manager.clone();

    let mut watcher =
        notify::recommended_watcher(move |res: Result<Event, notify::Error>| match res {
            Ok(event) => {
                let _ = tx.send(event);
            }
            Err(e) => error!("Watch error: {:?}", e),
        })?;

    watcher.watch(&args.config, RecursiveMode::NonRecursive)?;
    info!("Watching config file for changes: {:?}", args.config);

    tokio::spawn(async move {
        while let Some(_) = rx.recv().await {
            // Simple debounce: wait 200ms and drain
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            while rx.try_recv().is_ok() {}

            info!("Config file changed, reloading...");
            match AppConfig::load_from_file(&config_path) {
                Ok(new_config) => {
                    if let Err(e) = db_manager_reloader.update(&new_config) {
                        error!("Failed to update database configuration: {}", e);
                    } else {
                        info!("Database configuration updated successfully.");
                    }
                }
                Err(e) => {
                    error!("Failed to reload config: {}", e);
                }
            }
        }
    });

    if let Some(port) = args.port {
        info!("Starting MCP Server in HTTP mode on port {}", port);
        mcp::start_http_server(db_manager, port).await?;
    } else {
        info!("Starting MCP Server in Stdio mode...");
        mcp::start_stdio_server(db_manager).await?;
    }

    Ok(())
}
