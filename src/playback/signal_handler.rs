//! Platform-specific signal handling for playback proxy

#[cfg(unix)]
pub async fn wait_for_shutdown_signal() -> Result<(), std::io::Error> {
    use tokio::signal::unix::{SignalKind, signal};
    use tracing::info;

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    tokio::select! {
        _ = sigint.recv() => {
            info!("Received SIGINT, shutting down gracefully...");
            Ok(())
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM, shutting down gracefully...");
            Ok(())
        }
    }
}

#[cfg(windows)]
pub async fn wait_for_shutdown_signal() -> Result<(), std::io::Error> {
    use tokio::signal::windows;
    use tracing::info;

    // On Windows, listen for both CTRL_C and CTRL_BREAK events
    // CTRL_BREAK is the semantic equivalent of SIGTERM
    let mut ctrl_c = windows::ctrl_c()?;
    let mut ctrl_break = windows::ctrl_break()?;

    tokio::select! {
        _ = ctrl_c.recv() => {
            info!("Received CTRL_C, shutting down gracefully...");
            Ok(())
        }
        _ = ctrl_break.recv() => {
            info!("Received CTRL_BREAK (SIGTERM equivalent), shutting down gracefully...");
            Ok(())
        }
    }
}
