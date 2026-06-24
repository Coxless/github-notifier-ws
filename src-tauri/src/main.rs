#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod config;
mod github;
mod notify;
mod rules;
mod tray;

use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

pub struct AppState {
    pub config: config::Config,
    pub last_modified: Option<String>,
    pub server_poll_interval: u64,
    pub snooze_until: Option<std::time::Instant>,
    pub unread_count: usize,
    pub api_rate_remaining: Option<u32>,
    pub last_sync: Option<std::time::Instant>,
    pub wake_poll: Arc<tokio::sync::Notify>,
    pub thread_url_cache: HashMap<String, String>, // thread_id → subject.url
}

pub type AppStateHandle = Arc<Mutex<AppState>>;

#[tauri::command]
async fn set_github_token(token: String) -> Result<(), String> {
    github::store_token(&token).map_err(|e| e.to_string())
}

#[tauri::command]
async fn has_github_token() -> bool {
    github::get_stored_token().is_ok()
}

fn main() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            for arg in argv.iter().skip(1) {
                if arg.starts_with("github-notifier-ws://") {
                    let app = app.clone();
                    let url = arg.clone();
                    tauri::async_runtime::spawn(async move {
                        actions::handle_url(&app, &url).await;
                    });
                }
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![set_github_token, has_github_token])
        .setup(|app| {
            let config = config::load_or_default();
            let state: AppStateHandle = Arc::new(Mutex::new(AppState {
                config,
                last_modified: None,
                server_poll_interval: 60,
                snooze_until: None,
                unread_count: 0,
                api_rate_remaining: None,
                last_sync: None,
                wake_poll: Arc::new(tokio::sync::Notify::new()),
                thread_url_cache: HashMap::new(),
            }));

            app.manage(state.clone());

            // System tray
            tray::setup(app, state.clone())?;

            // First-run: show token setup window when no token is stored
            if github::get_stored_token().is_err() {
                tauri::WebviewWindowBuilder::new(
                    app,
                    "setup",
                    tauri::WebviewUrl::App("setup.html".into()),
                )
                .title("GitHub Token セットアップ — github-notifier-ws")
                .inner_size(520.0, 280.0)
                .resizable(false)
                .center()
                .build()?;
            }

            // Config file watcher
            let app_handle = app.handle().clone();
            let state2 = state.clone();
            tauri::async_runtime::spawn(async move {
                config::start_watcher(app_handle, state2).await;
            });

            // Register deep-link event handler
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let app_handle2 = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        let app = app_handle2.clone();
                        let url_str = url.to_string();
                        tauri::async_runtime::spawn(async move {
                            actions::handle_url(&app, &url_str).await;
                        });
                    }
                });
            }

            // Poll loop
            let app_handle3 = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                github::poll_loop(app_handle3, state.clone()).await;
            });

            Ok(())
        })
        .on_window_event(|_window, event| {
            // Prevent the app from exiting when windows are closed
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("github-notifier-ws の起動に失敗しました")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
