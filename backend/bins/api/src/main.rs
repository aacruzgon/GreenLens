//! The PDM `api` binary ([architecture 3]): every public endpoint, the worker API, the SSE hub,
//! the job broker, the scheduler and the deletion runner.
//!
//! `api` serves; `api openapi` prints the `OpenAPI` document (written to `backend/openapi.json`,
//! from which the app's TypeScript types are generated).

mod broker;
mod config;
mod deletion;
mod error;
mod events;
mod extract;
mod limits;
mod media;
mod openapi;
mod push;
mod reply;
mod revoke;
mod router;
mod routes;
mod scheduler;
mod sealing;
mod seams;
mod signing;
mod state;
mod uploads;
mod wiring;
mod workers;

#[cfg(test)]
mod tests;

use std::io::Write as _;

use anyhow::Context as _;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

/// Start-up in order: tracing; configuration; the ports (store with schema, blob, adapters,
/// models, the species photo index); resumed deletions; the scheduler; then serve until a signal,
/// and drain.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        let json = serde_json::to_string_pretty(&router::openapi())?;
        std::io::stdout().write_all(json.as_bytes())?;
        return Ok(());
    }
    let config = config::Config::from_env().context("configuration")?;
    init_tracing(config.log_json);
    tracing::info!(bind = %config.bind, "starting");

    let shutdown = CancellationToken::new();
    let tasks = TaskTracker::new();
    let drain = config.shutdown_drain;
    let bind = config.bind;
    let state = wiring::build(config, shutdown.clone(), tasks.clone()).await?;

    let resumed = state.deletions.resume().await.context("resume deletions")?;
    if resumed > 0 {
        tracing::info!(resumed, "deletions resumed");
    }
    scheduler::start(&state);
    let deletes = state.deletes.clone();
    tokio::spawn(signal(shutdown.clone()));

    let listener = tokio::net::TcpListener::bind(bind).await.context("bind")?;
    // Graceful shutdown waits for every open connection: the SSE streams and the claim
    // long-poll end on the token, so this returns soon after the signal.
    axum::serve(listener, router::app(state))
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .context("serve")?;

    // Background tasks get `SHUTDOWN_DRAIN_S`. Pending RustFS deletes get at least one full
    // attempt (the client's 120 s operation timeout) even when that drain is shorter, because an
    // interrupted delete can leak objects on RustFS 1.0.1 (storage 4).
    tasks.close();
    let delete_drain = drain.max(blob::client::OPERATION_TIMEOUT);
    let (tasks_waited, deletes_waited) = tokio::join!(
        tokio::time::timeout(drain, tasks.wait()),
        tokio::time::timeout(delete_drain, deletes.drain()),
    );
    if tasks_waited.is_err() {
        tracing::warn!(
            ?drain,
            "background tasks still running at exit; deletions resume on restart"
        );
    }
    if deletes_waited.is_err() {
        tracing::warn!(
            in_flight = deletes.in_flight(),
            "object deletes still running at exit"
        );
    }
    tracing::info!("stopped");
    Ok(())
}

/// JSON logs by default, `RUST_LOG` filtering (default `info`).
fn init_tracing(json: bool) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if json {
        builder.json().flatten_event(true).init();
    } else {
        builder.init();
    }
}

/// Cancels the token on Ctrl-C or SIGTERM (Docker's stop signal). A handler that fails to
/// install is logged and waits forever, so it can't end the process the moment it starts; the
/// other signal, or Docker's SIGKILL, still stops it.
async fn signal(shutdown: CancellationToken) {
    let ctrl_c = until_signal("ctrl_c", tokio::signal::ctrl_c());
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => until_signal("sigterm", std::future::ready(Err(e))).await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = term => {}
    }
    tracing::info!("shutdown signal");
    shutdown.cancel();
}

/// Waits for a signal. If its handler can't be installed, logs that and never returns.
async fn until_signal(name: &str, signal: impl Future<Output = std::io::Result<()>>) {
    if let Err(error) = signal.await {
        tracing::error!(signal = name, %error, "signal handler not installed");
        std::future::pending::<()>().await;
    }
}
