//! SCM (Service Control Manager) wrappers for starting and stopping the
//! FindAnythingWatcher service from the tray app.

use anyhow::{Context, Result};
use find_windows_service::SERVICE_NAME;
use windows_service::{
    service::{ServiceAccess, ServiceState},
    service_manager::{ServiceManager, ServiceManagerAccess},
};

/// Query whether the service is currently running.
pub fn is_service_running() -> bool {
    query_service_state()
        .map(|s| s == ServiceState::Running || s == ServiceState::StartPending)
        .unwrap_or(false)
}

/// Query the current service state.
pub fn query_service_state() -> Result<ServiceState> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("connecting to SCM")?;
    let service = manager
        .open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)
        .context("opening service")?;
    let status = service.query_status().context("querying service status")?;
    Ok(status.current_state)
}

/// Start the service.
pub fn start_service() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("connecting to SCM")?;
    let service = manager
        .open_service(SERVICE_NAME, ServiceAccess::START)
        .context("opening service for start")?;
    service.start(&[] as &[&str]).context("starting service")?;
    Ok(())
}

/// Stop the service (best-effort; does not wait for it to stop).
pub fn stop_service() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("connecting to SCM")?;
    let service = manager
        .open_service(SERVICE_NAME, ServiceAccess::STOP)
        .context("opening service for stop")?;
    service.stop().context("stopping service")?;
    Ok(())
}
