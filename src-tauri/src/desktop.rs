use std::{ffi::OsStr, sync::Mutex as StdMutex};

use serde::Serialize;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Emitter, Listener, Manager,
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_notification::{NotificationExt as _, PermissionState};
use tauri_plugin_updater::{Update, UpdaterExt as _};
use tokio::sync::Mutex;
use url::Url;

use crate::{
    preferences::{resolved_language, Language},
    state::AppState,
    windows,
};

const AUTOSTART_ARGUMENT: &str = "--autostart";
const UPDATE_ENDPOINT: &str =
    "https://github.com/likeratel/ratelline/releases/latest/download/latest.json";
const MENU_OPEN: &str = "show-panel";
const MENU_DASHBOARD: &str = "open-dashboard";
const MENU_REFRESH: &str = "refresh";
const MENU_QUIT: &str = "quit";
pub(crate) const TRAY_ID: &str = "main-tray";

pub(crate) struct DesktopState {
    updates: Mutex<UpdateCache>,
    tray_language: StdMutex<Language>,
}

#[derive(Default)]
struct UpdateCache {
    pending_update: Option<Update>,
    reusable_status: Option<UpdateStatus>,
    installed_version: Option<String>,
}

impl DesktopState {
    pub(crate) fn new(tray_language: Language) -> Self {
        Self {
            updates: Mutex::new(UpdateCache::default()),
            tray_language: StdMutex::new(tray_language),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopStatus {
    autostart: bool,
    notification_permission: NotificationPermission,
    updater_configured: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NotificationPermission {
    Granted,
    Denied,
    Prompt,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateStatus {
    available: bool,
    version: Option<String>,
    body: Option<String>,
}

pub(crate) fn autostart_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_autostart::Builder::new()
        .arg(AUTOSTART_ARGUMENT)
        .build()
}

pub(crate) fn updater_plugin<R: tauri::Runtime>(
) -> tauri::plugin::TauriPlugin<R, tauri_plugin_updater::Config> {
    let builder = tauri_plugin_updater::Builder::new();
    match updater_public_key() {
        Some(public_key) => builder.pubkey(public_key).build(),
        None => builder.build(),
    }
}

pub(crate) fn launched_by_autostart() -> bool {
    has_autostart_argument(std::env::args_os())
}

pub(crate) async fn initial_settings(state: &AppState) -> (Language, bool) {
    let snapshot = state.snapshot().await;
    (
        resolved_language(&snapshot.settings.preferences.language),
        snapshot.settings.preferences.check_updates_automatically,
    )
}

pub(crate) fn start_automatic_update_check(app: AppHandle) {
    if updater_public_key().is_none() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let result = {
            let state = app.state::<DesktopState>();
            check_update_inner(&app, state.inner(), CheckMode::Automatic).await
        };
        match result {
            Ok(status) => {
                if let Err(error) = app.emit("update-status", status) {
                    eprintln!("Failed to emit the automatic update status: {error}");
                }
            }
            Err(error) => {
                eprintln!("Automatic update check failed: {error}");
                if let Err(emit_error) = app.emit("update-error", error) {
                    eprintln!("Failed to emit the automatic update error: {emit_error}");
                }
            }
        }
    });
}

pub(crate) fn start_tray_language_sync(app: AppHandle) {
    let listener_app = app.clone();
    app.listen("snapshot-updated", move |event| {
        let snapshot = match serde_json::from_str::<crate::models::Snapshot>(event.payload()) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!("Failed to read the snapshot for tray localization: {error}");
                return;
            }
        };
        let language = resolved_language(&snapshot.settings.preferences.language);
        let state = listener_app.state::<DesktopState>();
        let mut current_language = match state.tray_language.lock() {
            Ok(language) => language,
            Err(error) => {
                eprintln!("Failed to lock the tray language state: {error}");
                return;
            }
        };
        if *current_language == language {
            return;
        }
        if let Err(error) = refresh_tray_menu(&listener_app, language) {
            eprintln!("Failed to refresh the localized tray menu: {error}");
        } else {
            *current_language = language;
        }
    });
}

pub(crate) fn build_tray(app: &App, language: Language) -> tauri::Result<()> {
    let menu = localized_menu(app, language)?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("ratelline")
        .menu(&menu)
        .icon_as_template(true)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Down,
                ..
            } => windows::begin_tray_panel_click(tray.app_handle()),
            TrayIconEvent::Click {
                position,
                rect,
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => windows::finish_tray_panel_click(tray.app_handle(), rect, position),
            TrayIconEvent::Leave { .. } => windows::cancel_tray_panel_click(tray.app_handle()),
            _ => {}
        });
    if let Some(icon) = tray_icon(app)? {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

/// macOS menu-bar glyph: black with alpha, rendered as a template image at the
/// 22pt menu-bar height (44px for Retina displays).
#[cfg(target_os = "macos")]
const MENU_BAR_TEMPLATE_ICON: &[u8] = include_bytes!("../icons/tray@2x.png");

/// The full-color app icon is a dark square that would render as a solid
/// block when used as a macOS template image, so macOS uses a dedicated glyph.
/// Other platforms keep the app icon because a black glyph is unreadable on
/// dark Windows and Linux trays.
fn tray_icon(app: &App) -> tauri::Result<Option<Image<'static>>> {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        Image::from_bytes(MENU_BAR_TEMPLATE_ICON).map(Some)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(app
            .default_window_icon()
            .map(|icon| icon.clone().to_owned()))
    }
}

/// Rebuilds the native tray menu after `Settings.preferences.language` changes.
pub(crate) fn refresh_tray_menu(app: &AppHandle, language: Language) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "The tray icon is unavailable.".to_owned())?;
    let menu = localized_menu(app, resolved_language(&language))
        .map_err(|error| format!("The localized tray menu could not be built: {error}"))?;
    tray.set_menu(Some(menu))
        .map_err(|error| format!("The localized tray menu could not be applied: {error}"))
}

#[tauri::command]
pub(crate) fn desktop_status(app: AppHandle) -> Result<DesktopStatus, String> {
    let autostart = app
        .autolaunch()
        .is_enabled()
        .map_err(|error| format!("Autostart status could not be read: {error}"))?;
    let permission = app
        .notification()
        .permission_state()
        .map_err(|error| format!("Notification permission could not be read: {error}"))?;
    Ok(DesktopStatus {
        autostart,
        notification_permission: map_permission(permission),
        updater_configured: updater_public_key().is_some(),
    })
}

#[tauri::command]
pub(crate) fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let manager = app.autolaunch();
    if enabled {
        manager
            .enable()
            .map_err(|error| format!("Autostart could not be enabled: {error}"))?;
    } else {
        manager
            .disable()
            .map_err(|error| format!("Autostart could not be disabled: {error}"))?;
    }
    manager
        .is_enabled()
        .map_err(|error| format!("Autostart status could not be verified: {error}"))
}

#[tauri::command]
pub(crate) fn request_notification_permission(
    app: AppHandle,
) -> Result<NotificationPermission, String> {
    app.notification()
        .request_permission()
        .map(map_permission)
        .map_err(|error| format!("Notification permission could not be requested: {error}"))
}

#[tauri::command]
pub(crate) async fn check_update(
    app: AppHandle,
    state: tauri::State<'_, DesktopState>,
    fresh: Option<bool>,
) -> Result<UpdateStatus, String> {
    let mode = if fresh.unwrap_or(false) {
        CheckMode::Fresh
    } else {
        CheckMode::ReuseAutomatic
    };
    check_update_inner(&app, state.inner(), mode).await
}

#[derive(Clone, Copy)]
enum CheckMode {
    Automatic,
    ReuseAutomatic,
    Fresh,
}

async fn check_update_inner(
    app: &AppHandle,
    state: &DesktopState,
    mode: CheckMode,
) -> Result<UpdateStatus, String> {
    let public_key = require_updater_public_key()?;
    let mut cache = state.updates.lock().await;
    if cache.installed_version.is_some() {
        return Ok(UpdateStatus {
            available: false,
            version: None,
            body: None,
        });
    }
    if matches!(mode, CheckMode::ReuseAutomatic) {
        if let Some(status) = cache.reusable_status.take() {
            return Ok(status);
        }
    }
    let endpoint = update_endpoint()?;
    let updater = app
        .updater_builder()
        .pubkey(public_key)
        .endpoints(vec![endpoint])
        .map_err(|error| format!("The update endpoint is invalid: {error}"))?
        .build()
        .map_err(|error| format!("The updater could not be initialized: {error}"))?;

    let update = updater
        .check()
        .await
        .map_err(|error| format!("The update check failed: {error}"))?;
    let status = update.as_ref().map_or(
        UpdateStatus {
            available: false,
            version: None,
            body: None,
        },
        |update| UpdateStatus {
            available: true,
            version: Some(update.version.clone()),
            body: update.body.clone(),
        },
    );
    cache.pending_update = update;
    cache.reusable_status = matches!(mode, CheckMode::Automatic).then(|| status.clone());
    Ok(status)
}

#[tauri::command]
pub(crate) async fn install_update(
    state: tauri::State<'_, DesktopState>,
    expected_version: String,
) -> Result<(), String> {
    let mut cache = state.updates.lock().await;
    validate_install_approval(
        &expected_version,
        cache
            .pending_update
            .as_ref()
            .map(|update| update.version.as_str()),
        cache.installed_version.as_deref(),
    )?;
    let update = cache.pending_update.as_ref().ok_or_else(|| {
        "No checked update is ready to install. Check for updates first.".to_owned()
    })?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| {
            format!("The signed update could not be downloaded or installed: {error}")
        })?;
    cache.installed_version = Some(update.version.clone());
    cache.pending_update = None;
    cache.reusable_status = None;
    Ok(())
}

fn validate_install_approval(
    expected: &str,
    pending: Option<&str>,
    installed: Option<&str>,
) -> Result<(), String> {
    if installed.is_some() {
        return Err(
            "An update has already been installed. Restart the application before updating again."
                .into(),
        );
    }
    match pending {
        None => Err("No checked update is ready to install. Check for updates first.".into()),
        Some(version) if version == expected => Ok(()),
        Some(_) => Err("The available update changed. Check for updates and approve the new version before installing.".into()),
    }
}

fn require_updater_public_key() -> Result<&'static str, String> {
    updater_public_key().ok_or_else(|| {
        "Updates are not configured in this build. Set RATELLINE_UPDATER_PUBLIC_KEY when building the application."
            .to_owned()
    })
}

fn updater_public_key() -> Option<&'static str> {
    option_env!("RATELLINE_UPDATER_PUBLIC_KEY")
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn update_endpoint() -> Result<Url, String> {
    Url::parse(UPDATE_ENDPOINT).map_err(|error| error.to_string())
}

fn map_permission(permission: PermissionState) -> NotificationPermission {
    match permission {
        PermissionState::Granted => NotificationPermission::Granted,
        PermissionState::Denied => NotificationPermission::Denied,
        PermissionState::Prompt | PermissionState::PromptWithRationale => {
            NotificationPermission::Prompt
        }
    }
}

fn has_autostart_argument<I, S>(arguments: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    arguments
        .into_iter()
        .any(|argument| argument.as_ref() == OsStr::new(AUTOSTART_ARGUMENT))
}

fn localized_menu<R: tauri::Runtime>(
    manager: &impl Manager<R>,
    language: Language,
) -> tauri::Result<Menu<R>> {
    let labels = TrayLabels::for_language(language);
    let open = MenuItem::with_id(manager, MENU_OPEN, labels.open, true, None::<&str>)?;
    let dashboard = MenuItem::with_id(
        manager,
        MENU_DASHBOARD,
        labels.dashboard,
        true,
        None::<&str>,
    )?;
    let refresh = MenuItem::with_id(manager, MENU_REFRESH, labels.refresh, true, None::<&str>)?;
    let quit = MenuItem::with_id(manager, MENU_QUIT, labels.quit, true, None::<&str>)?;
    let menu = Menu::new(manager)?;
    menu.append_items(&[&open, &dashboard, &refresh, &quit])?;
    Ok(menu)
}

fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        MENU_OPEN => windows::show_panel(app, None),
        MENU_DASHBOARD => {
            if let Err(error) = windows::open_dashboard(app) {
                eprintln!("Failed to open the dashboard from the tray: {error}");
            }
        }
        MENU_REFRESH => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                state.refresh(&app).await;
            });
        }
        MENU_QUIT => app.exit(0),
        _ => {}
    }
}

struct TrayLabels {
    open: &'static str,
    dashboard: &'static str,
    refresh: &'static str,
    quit: &'static str,
}

impl TrayLabels {
    fn for_language(language: Language) -> Self {
        match language {
            Language::Korean => Self {
                open: "패널 보기",
                dashboard: "대시보드 열기",
                refresh: "새로고침",
                quit: "종료",
            },
            Language::Japanese => Self {
                open: "パネルを表示",
                dashboard: "ダッシュボードを開く",
                refresh: "更新",
                quit: "終了",
            },
            Language::SimplifiedChinese => Self {
                open: "显示面板",
                dashboard: "打开仪表板",
                refresh: "刷新",
                quit: "退出",
            },
            Language::System | Language::English => Self {
                open: "Show Panel",
                dashboard: "Open Dashboard",
                refresh: "Refresh",
                quit: "Quit",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    #[test]
    fn menu_bar_icon_is_a_retina_template_glyph() {
        let icon = super::Image::from_bytes(super::MENU_BAR_TEMPLATE_ICON).unwrap();
        // 22pt tall at 2x; the side-profile ratel is wider than it is tall.
        assert_eq!(icon.height(), 44);
        assert!((44..=88).contains(&icon.width()), "width {}", icon.width());
        let (pixels, _) = icon.rgba().as_chunks::<4>();
        assert!(pixels
            .iter()
            .all(|pixel| pixel[3] == 0 || pixel[..3] == [0, 0, 0]));
        assert!(pixels.iter().any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn bundled_updater_configuration_deserializes_without_a_signing_key() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater: tauri_plugin_updater::Config =
            serde_json::from_value(config["plugins"]["updater"].clone()).unwrap();
        assert!(updater.pubkey.is_empty());
        assert!(updater.endpoints.is_empty());
    }

    use super::*;

    #[test]
    fn installation_requires_current_version_approval_and_restart_after_success() {
        assert!(validate_install_approval("1.2.0", Some("1.2.0"), None).is_ok());
        assert!(validate_install_approval("1.1.0", Some("1.2.0"), None).is_err());
        assert!(validate_install_approval("", Some("1.2.0"), None).is_err());
        assert!(validate_install_approval("1.2.0", None, None).is_err());
        assert!(validate_install_approval("1.2.0", Some("1.2.0"), Some("1.2.0")).is_err());
    }

    #[test]
    fn detects_only_the_exact_autostart_argument() {
        assert!(has_autostart_argument(["ratelline", "--autostart"]));
        assert!(!has_autostart_argument(["ratelline", "--autostart=true"]));
        assert!(!has_autostart_argument(["ratelline"]));
    }

    #[test]
    fn updater_endpoint_is_fixed_and_secure() {
        let endpoint = update_endpoint().expect("valid update endpoint");
        assert_eq!(endpoint.scheme(), "https");
        assert_eq!(endpoint.host_str(), Some("github.com"));
        assert_eq!(
            endpoint.path(),
            "/likeratel/ratelline/releases/latest/download/latest.json"
        );
    }

    #[test]
    fn tray_labels_cover_supported_languages() {
        assert_eq!(TrayLabels::for_language(Language::Korean).quit, "종료");
        assert_eq!(TrayLabels::for_language(Language::English).quit, "Quit");
        assert_eq!(TrayLabels::for_language(Language::Japanese).quit, "終了");
        assert_eq!(
            TrayLabels::for_language(Language::SimplifiedChinese).quit,
            "退出"
        );
    }
}
