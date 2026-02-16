//! Windows Service support for find-watch.
//!
//! Provides `install_service` and `uninstall_service` for managing the
//! `FindAnythingWatcher` Windows Service.
//!
//! The `service_main` entry point lives in `find-watch`'s `watch_main.rs`
//! because `define_windows_service!` emits a public FFI symbol that must
//! reside in the binary crate.

#![cfg(windows)]

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use windows_service::{
    service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState,
        ServiceType,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
};
use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

pub const SERVICE_NAME: &str = "FindAnythingWatcher";
const SERVICE_DISPLAY_NAME: &str = "Find Anything Watcher";
const SERVICE_DESCRIPTION: &str =
    "Find Anything file watcher \u{2014} keeps the index current";
const REGISTRY_RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const REGISTRY_VALUE_NAME: &str = "FindAnythingTray";

/// Register the Find Anything watcher as a Windows Service and add the tray
/// app to the current user's startup run key.
///
/// Requires Administrator privileges.
pub fn install_service(config_path: &Path, service_name: &str) -> Result<()> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CREATE_SERVICE,
    )
    .context("opening Service Control Manager (run as administrator)")?;

    let current_exe = std::env::current_exe().context("resolving current executable path")?;

    let config_abs = config_path
        .canonicalize()
        .unwrap_or_else(|_| config_path.to_path_buf());

    let service_info = ServiceInfo {
        name: OsString::from(service_name),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: current_exe.clone(),
        launch_arguments: vec![
            OsString::from("service-run"),
            OsString::from("--config"),
            config_abs.clone().into_os_string(),
        ],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };

    let service = manager
        .create_service(&service_info, ServiceAccess::CHANGE_CONFIG)
        .context("creating Windows service")?;

    service
        .set_description(SERVICE_DESCRIPTION)
        .context("setting service description")?;

    // Register tray app in HKCU Run so it starts at user login.
    let tray_exe = current_exe
        .parent()
        .map(|p| p.join("find-tray.exe"))
        .unwrap_or_else(|| std::path::PathBuf::from("find-tray.exe"));

    let run_value = format!(
        "\"{}\" --config \"{}\"",
        tray_exe.display(),
        config_abs.display()
    );

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(REGISTRY_RUN_KEY, KEY_SET_VALUE)
        .context("opening HKCU Run registry key")?;
    run_key
        .set_value(REGISTRY_VALUE_NAME, &run_value)
        .context("writing tray app to Run registry")?;

    println!("Service '{service_name}' installed successfully.");
    println!("Tray app registered to start at login: {run_value}");
    println!();
    println!("Start the service now with:");
    println!("  sc start {service_name}");
    println!("Or reboot for auto-start.");

    Ok(())
}

/// Stop and delete the Find Anything watcher service, and remove the tray
/// app from the current user's startup run key.
///
/// Requires Administrator privileges.
pub fn uninstall_service(service_name: &str) -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("opening Service Control Manager (run as administrator)")?;

    let service = manager
        .open_service(
            service_name,
            ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
        )
        .context("opening service (is it installed?)")?;

    // Stop the service if it's running.
    let status = service.query_status().context("querying service status")?;
    if status.current_state != ServiceState::Stopped
        && status.current_state != ServiceState::StopPending
    {
        service.stop().context("sending stop signal to service")?;

        // Wait up to 30 seconds for the service to stop.
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            std::thread::sleep(Duration::from_millis(500));
            let s = service.query_status().context("querying service status")?;
            if s.current_state == ServiceState::Stopped {
                break;
            }
            if std::time::Instant::now() > deadline {
                anyhow::bail!("timed out waiting for service '{service_name}' to stop");
            }
        }
    }

    service.delete().context("deleting service")?;

    // Remove tray app from HKCU Run.
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run_key) = hkcu.open_subkey_with_flags(REGISTRY_RUN_KEY, KEY_SET_VALUE) {
        let _ = run_key.delete_value(REGISTRY_VALUE_NAME);
    }

    println!("Service '{service_name}' uninstalled.");
    println!("Tray app startup entry removed.");

    Ok(())
}
