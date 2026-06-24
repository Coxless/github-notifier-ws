use std::time::{Duration, Instant};
use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle,
};

pub fn setup(app: &mut tauri::App, state: crate::AppStateHandle) -> anyhow::Result<()> {
    let menu = build_menu(app)?;

    TrayIconBuilder::with_id("main")
        .tooltip("github-notifier-ws")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                open_github_notifications(app);
            }
        })
        .on_menu_event(move |app, event| {
            let state = state.clone();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                handle_menu_event(&app, &state, event.id().as_ref()).await;
            });
        })
        .build(app)?;

    Ok(())
}

fn build_menu(app: &mut tauri::App) -> anyhow::Result<Menu<tauri::Wry>> {
    let check_now = MenuItemBuilder::with_id("check_now", "今すぐ確認").build(app)?;

    let snooze_30m = MenuItemBuilder::with_id("snooze_30m", "30分").build(app)?;
    let snooze_1h = MenuItemBuilder::with_id("snooze_1h", "1時間").build(app)?;
    let snooze_tomorrow = MenuItemBuilder::with_id("snooze_tomorrow", "明日まで止める").build(app)?;
    let snooze_cancel = MenuItemBuilder::with_id("snooze_cancel", "一時停止を解除").build(app)?;

    let snooze_sub = SubmenuBuilder::with_id(app, "snooze", "通知を一時停止")
        .item(&snooze_30m)
        .item(&snooze_1h)
        .item(&snooze_tomorrow)
        .separator()
        .item(&snooze_cancel)
        .build()?;

    let open_config = MenuItemBuilder::with_id("open_config", "設定ファイルを開く").build(app)?;
    let reload_config = MenuItemBuilder::with_id("reload_config", "設定を再読み込み").build(app)?;
    let open_github = MenuItemBuilder::with_id("open_github", "GitHubで開く").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "終了").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&check_now)
        .item(&snooze_sub)
        .separator()
        .item(&open_config)
        .item(&reload_config)
        .separator()
        .item(&open_github)
        .separator()
        .item(&quit)
        .build()?;

    Ok(menu)
}

async fn handle_menu_event(
    app: &AppHandle,
    state: &crate::AppStateHandle,
    id: &str,
) {
    match id {
        "check_now" => {
            // Force immediate poll by resetting last_modified
            let mut s = state.lock().await;
            s.last_modified = None;
        }
        "snooze_30m" => set_snooze(state, Duration::from_secs(30 * 60)).await,
        "snooze_1h" => set_snooze(state, Duration::from_secs(60 * 60)).await,
        "snooze_tomorrow" => {
            // Until next day 09:00 — approximate: 16h from now
            set_snooze(state, Duration::from_secs(16 * 60 * 60)).await;
        }
        "snooze_cancel" => {
            let mut s = state.lock().await;
            s.snooze_until = None;
            set_idle(app).await;
        }
        "open_config" => open_config_file(app),
        "reload_config" => reload_config(app, state).await,
        "open_github" => open_github_notifications(app),
        "quit" => app.exit(0),
        _ => {}
    }
}

async fn set_snooze(state: &crate::AppStateHandle, duration: Duration) {
    let mut s = state.lock().await;
    s.snooze_until = Some(Instant::now() + duration);
}

fn open_github_notifications(app: &AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    let _ = app.opener().open_url("https://github.com/notifications", None::<&str>);
}

fn open_config_file(app: &AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    let path = crate::config::config_path();
    // Ensure it exists
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
    let _ = app.opener().open_path(path.to_string_lossy().as_ref(), None::<&str>);
}

async fn reload_config(app: &AppHandle, state: &crate::AppStateHandle) {
    let path = crate::config::config_path();
    match crate::config::load(&path) {
        Ok(new_config) => {
            let count = new_config.rules.len();
            let mut s = state.lock().await;
            s.config = new_config;
            drop(s);
            crate::notify::show_reload_success(app, count);
        }
        Err(e) => crate::notify::show_config_error(app, &e.to_string()),
    }
}

// ── Tray state helpers ────────────────────────────────────────────────────────

pub async fn set_idle(app: &AppHandle) {
    update_tooltip(app, "github-notifier-ws — 未読なし");
}

pub async fn set_unread(app: &AppHandle, count: usize) {
    update_tooltip(app, &format!("github-notifier-ws — {count} 件の未読"));
}

pub async fn set_error(app: &AppHandle, msg: &str) {
    update_tooltip(app, &format!("github-notifier-ws — エラー: {msg}"));
}

#[allow(dead_code)]
pub async fn set_paused(app: &AppHandle) {
    update_tooltip(app, "github-notifier-ws — 一時停止中");
}

fn update_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(text));
    }
}
