#![recursion_limit = "256"]
mod affectation;
mod auth;
mod cleanup;
mod config;
mod db;
mod demo;
mod demo_portal;
mod economic_import;
mod error;
mod machine_soupe;
mod mdns;
mod models;
mod routes;
mod templates;
mod vocal;

use axum::middleware;
use config::Config;
use routes::AppState;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use std::str::FromStr;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "eo_suivi_elevage=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    if demo_portal::enabled() && !config.db_path.exists() {
        anyhow::ensure!(
            std::env::var("EO_DEMO_ADMIN_PASSWORD")
                .unwrap_or_default()
                .len()
                >= 16,
            "Mot de passe administrateur de démonstration requis (16 caractères minimum)."
        );
    }
    let db_url = format!("sqlite://{}", config.db_path.display());
    let options = SqliteConnectOptions::from_str(&db_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    if demo_portal::enabled() {
        demo_portal::verify_database(&pool).await?;
    }
    db::init(&pool).await?;
    if demo_portal::enabled() {
        demo_portal::init(&pool).await?;
        let cleanup_pool = pool.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                if let Err(e) = demo_portal::prune(&cleanup_pool).await {
                    tracing::error!(error=%e,"Nettoyage démo impossible");
                }
            }
        });
    }
    // Annonce sur le réseau local, pour que l'application Android trouve le
    // serveur sans qu'on lui saisisse une adresse. Le démon doit rester vivant
    // aussi longtemps que le serveur : le lier ici, et non dans une fonction
    // dont il sortirait, sinon l'annonce disparaît aussitôt publiée.
    let nom_elevage: Option<String> =
        sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle='nom_elevage'")
            .fetch_optional(&pool)
            .await?
            .flatten();
    let _annonce_reseau = mdns::annoncer(
        config.bind.port(),
        nom_elevage.as_deref(),
        env!("CARGO_PKG_VERSION"),
    );

    // Rétention des enregistrements vocaux : l'audio ne sert qu'au diagnostic
    // des dictées mal comprises, il s'efface seul au bout du délai prévu.
    let purge_pool = pool.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = routes::vocal::purger_audio(&purge_pool).await {
                tracing::error!(error=%e,"Purge des enregistrements vocaux impossible");
            }
            tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
        }
    });

    let templates = templates::build()?;
    let state = AppState::new(config.clone(), pool, templates);

    let app = routes::router(state.clone())
        .nest_service("/static", ServeDir::new("static"))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn_with_state(state, auth::guard));

    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(address = %config.bind, database = %config.db_path.display(), "EO-Suivi Rust démarré");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(%error, "écoute du signal Ctrl+C impossible");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::warn!(%error, "écoute du signal SIGTERM impossible"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
