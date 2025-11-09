//! Platform-specific signal handling for recording proxy

#[cfg(unix)]
pub async fn wait_for_shutdown_signal() -> Result<(), std::io::Error> {
    tokio::signal::ctrl_c().await
}

#[cfg(windows)]
pub async fn wait_for_shutdown_signal() -> Result<(), std::io::Error> {
    use tokio::signal::windows;

    // On Windows, listen for both CTRL_C and CTRL_BREAK events
    let mut ctrl_c = windows::ctrl_c()?;
    let mut ctrl_break = windows::ctrl_break()?;

    tokio::select! {
        _ = ctrl_c.recv() => Ok(()),
        _ = ctrl_break.recv() => Ok(()),
    }
}
