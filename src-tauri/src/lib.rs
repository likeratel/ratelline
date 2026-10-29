mod cli;
mod cloudflare_credentials;
mod commands;
mod credential_store;
mod credentials;
mod desktop;
mod legacy_keyring;
mod legacy_migration;
mod login;
pub mod models;
mod notifications;
mod nvm;
mod panel;
mod preferences;
pub mod providers;
mod settings;
mod state;
mod tray_animation;
mod windows;

use tauri::{Manager, RunEvent, WindowEvent};

use state::AppState;

pub(crate) use desktop::TRAY_ID;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let launched_by_autostart = desktop::launched_by_autostart();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(desktop::autostart_plugin())
        .plugin(tauri_plugin_notification::init())
        .plugin(desktop::updater_plugin())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let app_data_dir = app.path().app_data_dir()?;
            legacy_migration::migrate_app_data_dir(&app_data_dir);
            let credentials = credential_store::configure(&app_data_dir);
            let resource_dir = app.path().resource_dir().ok();
            let state = tauri::async_runtime::block_on(AppState::initialize(
                app_data_dir,
                resource_dir.as_deref(),
            ));
            let (language, check_updates_automatically) =
                tauri::async_runtime::block_on(desktop::initial_settings(&state));
            let accounts = tauri::async_runtime::block_on(state.snapshot())
                .settings
                .accounts;
            start_keychain_move(credentials, accounts);
            app.manage(state);
            app.manage(desktop::DesktopState::new(language));
            app.manage(windows::PanelFocusState::default());
            #[cfg(target_os = "macos")]
            app.manage(tray_animation::TrayAnimator::default());
            desktop::build_tray(app, language)?;
            desktop::start_tray_language_sync(app.handle().clone());
            state::start_polling(app.handle().clone());
            if check_updates_automatically {
                desktop::start_automatic_update_check(app.handle().clone());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != windows::MAIN_LABEL {
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = windows::hide_panel(window.app_handle());
                }
                WindowEvent::Focused(focused) => {
                    // Tao queues focus events on macOS, so the current state can be
                    // newer than this event when a hidden panel is shown again.
                    let currently_focused = *focused || window.is_focused().unwrap_or(true);
                    let should_hide = window
                        .app_handle()
                        .state::<windows::PanelFocusState>()
                        .should_hide_for_focus_change(*focused, currently_focused);
                    if should_hide {
                        let _ = windows::hide_panel(window.app_handle());
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::load_more,
            commands::save_settings,
            commands::change_settings,
            commands::refresh,
            commands::request_refresh,
            commands::discover,
            commands::create_connection,
            commands::connect_bitbucket,
            commands::connect_cloudflare,
            commands::register_eas_project,
            commands::get_github_cli_accounts,
            commands::connect_github_cli_account,
            commands::remove_connection,
            commands::begin_login,
            commands::get_login_status,
            commands::open_external,
            commands::hide_panel,
            commands::open_dashboard,
            desktop::desktop_status,
            desktop::set_autostart,
            desktop::request_notification_permission,
            desktop::check_update,
            desktop::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("ratelline failed to build")
        .run(move |app, event| match event {
            RunEvent::Ready if !launched_by_autostart => windows::show_panel(app, None),
            #[cfg(target_os = "macos")]
            // Preserve an existing visible dashboard instead of stealing focus
            // into the status panel on an application reopen event.
            RunEvent::Reopen { .. } if windows::should_show_panel_on_reopen(app) => {
                windows::show_panel(app, None)
            }
            _ => {}
        });
}

/// Moves Bitbucket and Cloudflare tokens from the keychain to the credentials
/// file without blocking startup. Credential lookups wait until it finishes, so
/// the first refresh cannot report a token that is still being moved.
fn start_keychain_move(
    store: &'static credential_store::CredentialFile,
    accounts: Vec<models::Account>,
) {
    tauri::async_runtime::spawn(async move {
        let moved =
            tokio::task::spawn_blocking(move || legacy_keyring::move_connections(store, &accounts))
                .await;
        if moved.is_err() {
            eprintln!("Moving credentials from the keychain was interrupted.");
        }
        credential_store::mark_ready();
    });
}
