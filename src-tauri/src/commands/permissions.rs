use tauri::{command, AppHandle, Runtime};

use crate::desktop_permissions::{
    get_desktop_permission_snapshot,
    open_desktop_permission_settings as open_desktop_permission_settings_impl,
    request_desktop_notification_permission as request_notification_permission_snapshot,
    DesktopPermissionSnapshot,
};

#[command]
pub async fn get_desktop_permission_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<DesktopPermissionSnapshot, String> {
    Ok(get_desktop_permission_snapshot(&app))
}

#[command]
pub async fn request_desktop_notification_permission<R: Runtime>(
    app: AppHandle<R>,
) -> Result<DesktopPermissionSnapshot, String> {
    request_notification_permission_snapshot(&app)
}

#[command]
pub async fn open_desktop_permission_settings(permission_kind: String) -> Result<(), String> {
    open_desktop_permission_settings_impl(&permission_kind)
}
