use tauri::command;

use crate::desktop_permissions::{get_desktop_permission_snapshot, DesktopPermissionSnapshot};

#[command]
pub async fn get_desktop_permission_status() -> Result<DesktopPermissionSnapshot, String> {
    Ok(get_desktop_permission_snapshot())
}
