use std::time::{Duration, Instant};
use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle,
};

const ICON_IDLE: &[u8] = include_bytes!("../icons/state_idle.png");
const ICON_UNREAD: &[u8] = include_bytes!("../icons/state_unread.png");
const ICON_ERROR: &[u8] = include_bytes!("../icons/state_error.png");
const ICON_PAUSED: &[u8] = include_bytes!("../icons/state_paused.png");

pub fn setup(app: &mut tauri::App, state: crate::AppStateHandle) -> anyhow::Result<()> {
    let menu = build_menu(app, 0, None, None, None)?;

    TrayIconBuilder::with_id("main")
        .tooltip("github-notifier-ws")
        .icon(tauri::image::Image::from_bytes(ICON_IDLE)?)
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

fn format_last_sync(last_sync: Option<Instant>) -> String {
    match last_sync {
        None => "未同期".to_string(),
        Some(t) => {
            let secs = t.elapsed().as_secs();
            if secs < 60 {
                format!("{}秒前に同期", secs)
            } else {
                format!("{}分前に同期", secs / 60)
            }
        }
    }
}

fn build_menu(
    app: &impl tauri::Manager<tauri::Wry>,
    unread_count: usize,
    last_sync: Option<Instant>,
    api_rate: Option<u32>,
    snooze_until: Option<Instant>,
) -> anyhow::Result<Menu<tauri::Wry>> {
    // Status line 1: unread count
    let unread_text = if unread_count == 0 {
        "  未読なし".to_string()
    } else {
        format!("  ● {} 件の未読", unread_count)
    };
    let status_unread = MenuItemBuilder::with_id("status_unread", &unread_text)
        .enabled(false)
        .build(app)?;

    // Status line 2: sync time + API rate
    let sync_text = {
        let sync = format_last_sync(last_sync);
        match api_rate {
            Some(r) => format!("  {}  ·  API残 {}", sync, r),
            None => format!("  {}", sync),
        }
    };
    let status_sync = MenuItemBuilder::with_id("status_sync", &sync_text)
        .enabled(false)
        .build(app)?;

    let check_now = MenuItemBuilder::with_id("check_now", "今すぐ確認").build(app)?;

    // Snooze submenu — label shows remaining time when active
    let snooze_label = match snooze_until {
        Some(until) if until > Instant::now() => {
            let mins = until.duration_since(Instant::now()).as_secs() / 60;
            if mins < 60 {
                format!("通知を一時停止中 (残 {}分)", mins)
            } else {
                format!("通知を一時停止中 (残 {}時間)", mins / 60)
            }
        }
        _ => "通知を一時停止".to_string(),
    };

    let snooze_30m = MenuItemBuilder::with_id("snooze_30m", "30分").build(app)?;
    let snooze_1h = MenuItemBuilder::with_id("snooze_1h", "1時間").build(app)?;
    let snooze_tomorrow =
        MenuItemBuilder::with_id("snooze_tomorrow", "明日まで止める").build(app)?;
    let snooze_cancel =
        MenuItemBuilder::with_id("snooze_cancel", "一時停止を解除").build(app)?;
    let snooze_sub = SubmenuBuilder::with_id(app, "snooze", &snooze_label)
        .item(&snooze_30m)
        .item(&snooze_1h)
        .item(&snooze_tomorrow)
        .separator()
        .item(&snooze_cancel)
        .build()?;

    let open_config =
        MenuItemBuilder::with_id("open_config", "設定ファイルを開く").build(app)?;
    let reload_config =
        MenuItemBuilder::with_id("reload_config", "設定を再読み込み").build(app)?;
    let open_github = MenuItemBuilder::with_id("open_github", "GitHubで開く").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "終了").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&status_unread)
        .item(&status_sync)
        .separator()
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

/// Rebuild the tray menu from current state and apply it to the tray icon.
pub async fn rebuild_menu(app: &AppHandle, state: &crate::AppStateHandle) {
    let (unread_count, last_sync, api_rate, snooze_until) = {
        let s = state.lock().await;
        (s.unread_count, s.last_sync, s.api_rate_remaining, s.snooze_until)
    };

    match build_menu(app, unread_count, last_sync, api_rate, snooze_until) {
        Ok(menu) => {
            if let Some(tray) = app.tray_by_id("main") {
                let _ = tray.set_menu(Some(menu));
            }
        }
        Err(e) => log::error!("メニュー構築エラー: {}", e),
    }
}

async fn handle_menu_event(app: &AppHandle, state: &crate::AppStateHandle, id: &str) {
    match id {
        "check_now" => {
            let wake = {
                let mut s = state.lock().await;
                s.last_modified = None;
                s.wake_poll.clone()
            };
            wake.notify_one();
        }
        "snooze_30m" => set_snooze(app, state, Duration::from_secs(30 * 60)).await,
        "snooze_1h" => set_snooze(app, state, Duration::from_secs(60 * 60)).await,
        "snooze_tomorrow" => {
            let duration = next_day_9am();
            set_snooze(app, state, duration).await;
        }
        "snooze_cancel" => {
            {
                let mut s = state.lock().await;
                s.snooze_until = None;
            }
            set_idle(app, state).await;
        }
        "open_config" => open_config_file(app),
        "reload_config" => reload_config(app, state).await,
        "open_github" => open_github_notifications(app),
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Calculate duration until the next 9:00 AM local time.
fn next_day_9am() -> Duration {
    use chrono::{Datelike, Duration as ChronoDuration, Local, TimeZone};

    let now = Local::now();
    // Try today 9am first; if already past, use tomorrow 9am
    let target = {
        let today_9am = Local
            .with_ymd_and_hms(now.year(), now.month(), now.day(), 9, 0, 0)
            .single();

        match today_9am {
            Some(t) if t > now => t,
            _ => {
                let tomorrow = now.date_naive() + ChronoDuration::days(1);
                Local
                    .with_ymd_and_hms(
                        tomorrow.year(),
                        tomorrow.month(),
                        tomorrow.day(),
                        9,
                        0,
                        0,
                    )
                    .single()
                    .unwrap_or_else(|| now + ChronoDuration::hours(16))
            }
        }
    };

    let secs = target
        .signed_duration_since(now)
        .num_seconds()
        .max(3600) as u64;
    Duration::from_secs(secs)
}

async fn set_snooze(app: &AppHandle, state: &crate::AppStateHandle, duration: Duration) {
    {
        let mut s = state.lock().await;
        s.snooze_until = Some(Instant::now() + duration);
    }
    set_paused(app, state).await;
}

fn open_github_notifications(app: &AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    let _ = app
        .opener()
        .open_url("https://github.com/notifications", None::<&str>);
}

fn open_config_file(app: &AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    let path = crate::config::config_path();
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, crate::config::DEFAULT_CONFIG);
    }
    let _ = app
        .opener()
        .open_path(path.to_string_lossy().as_ref(), None::<&str>);
}

async fn reload_config(app: &AppHandle, state: &crate::AppStateHandle) {
    let path = crate::config::config_path();
    match crate::config::load(&path) {
        Ok(new_config) => {
            let count = new_config.rules.len();
            {
                let mut s = state.lock().await;
                s.config = new_config;
            }
            crate::notify::show_reload_success(app, count);
        }
        Err(e) => crate::notify::show_config_error(app, &e.to_string()),
    }
}

// ── Tray state helpers ────────────────────────────────────────────────────────

fn set_icon(app: &AppHandle, icon_bytes: &[u8]) {
    if let Some(tray) = app.tray_by_id("main") {
        match tauri::image::Image::from_bytes(icon_bytes) {
            Ok(img) => {
                let _ = tray.set_icon(Some(img));
            }
            Err(e) => log::error!("アイコン設定エラー: {}", e),
        }
    }
}

pub async fn set_idle(app: &AppHandle, state: &crate::AppStateHandle) {
    set_icon(app, ICON_IDLE);
    update_tooltip(app, "github-notifier-ws — 未読なし");
    rebuild_menu(app, state).await;
}

pub async fn set_unread(app: &AppHandle, state: &crate::AppStateHandle, count: usize) {
    set_icon(app, ICON_UNREAD);
    update_tooltip(app, &format!("github-notifier-ws — {} 件の未読", count));
    rebuild_menu(app, state).await;
}

pub async fn set_error(app: &AppHandle, state: &crate::AppStateHandle, msg: &str) {
    set_icon(app, ICON_ERROR);
    update_tooltip(app, &format!("github-notifier-ws — エラー: {}", msg));
    rebuild_menu(app, state).await;
}

pub async fn set_paused(app: &AppHandle, state: &crate::AppStateHandle) {
    set_icon(app, ICON_PAUSED);
    update_tooltip(app, "github-notifier-ws — 一時停止中");
    rebuild_menu(app, state).await;
}

fn update_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(text));
    }
}
