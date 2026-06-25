use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use url::Url;

/// Parse and dispatch a github-notifier-ws:// deep link URL.
/// Called both from deep-link plugin events and from single-instance handler.
pub async fn handle_url(app: &AppHandle, url: &str) {
    log::info!("deep link: {}", url);

    let Ok(parsed) = Url::parse(url) else {
        log::warn!("deep link parse error: {}", url);
        return;
    };

    let do_param = parsed
        .query_pairs()
        .find(|(k, _)| k == "do")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();

    match parsed.host_str() {
        Some("thread") => {
            let thread_id = parsed.path().trim_start_matches('/').to_string();
            let repo = parsed
                .query_pairs()
                .find(|(k, _)| k == "repo")
                .map(|(_, v)| v.into_owned());
            handle_thread(app, &thread_id, &do_param, repo.as_deref()).await;
        }
        Some("inbox") => handle_inbox(app, &do_param).await,
        other => log::warn!("unknown deep link host: {:?}", other),
    }
}

async fn handle_thread(app: &AppHandle, thread_id: &str, action: &str, repo: Option<&str>) {
    match action {
        "open" => {
            // Resolve the specific thread URL from cache; fall back to inbox
            let html_url = resolve_thread_html_url(app, thread_id).await;
            open_url(app, &html_url);
        }
        "mark_read" => {
            let token = match crate::github::get_stored_token() {
                Ok(t) => t,
                Err(e) => {
                    log::error!("mark_read: トークン取得失敗: {}", e);
                    return;
                }
            };
            let client = crate::github::GitHubClient::new(token);
            if let Err(e) = client.mark_read(thread_id).await {
                log::error!("mark_read 失敗: {}", e);
            }
        }
        "mute" => {
            let repo = match repo {
                Some(r) => r.to_string(),
                None => {
                    log::warn!("mute: repo パラメータがありません");
                    return;
                }
            };
            match crate::config::append_ignore_rule(&repo) {
                Ok(()) => {
                    log::info!("mute: {} を設定ファイルに追記しました", repo);
                    // The file watcher will pick up the change and show a reload toast
                }
                Err(e) => {
                    log::error!("mute: 設定ファイルへの書き込み失敗: {}", e);
                    crate::notify::show_config_error(app, &format!("ミュートルールの追記に失敗: {}", e));
                }
            }
        }
        _ => log::warn!("unknown thread action: {}", action),
    }
}

async fn handle_inbox(app: &AppHandle, action: &str) {
    match action {
        "open" => open_url(app, "https://github.com/notifications"),
        "mark_all_read" => {
            // Destructive: check allow_destructive before proceeding
            let allowed = {
                use tauri::Manager;
                let state = app.state::<crate::AppStateHandle>();
                let s = state.lock().await;
                s.config.allow_destructive
            };

            if !allowed {
                log::info!(
                    "mark_all_read をスキップ: config の allow_destructive が false です"
                );
                // Open the inbox so the user can manually decide
                open_url(app, "https://github.com/notifications");
                return;
            }

            let token = match crate::github::get_stored_token() {
                Ok(t) => t,
                Err(e) => {
                    log::error!("mark_all_read: トークン取得失敗: {}", e);
                    return;
                }
            };
            let client = crate::github::GitHubClient::new(token);
            match client.mark_all_read().await {
                Ok(()) => log::info!("すべて既読化完了"),
                Err(e) => log::error!("mark_all_read 失敗: {}", e),
            }
        }
        _ => log::warn!("unknown inbox action: {}", action),
    }
}

/// Resolve a thread's HTML URL from the in-memory cache.
/// Returns the GitHub notifications inbox URL as fallback.
async fn resolve_thread_html_url(app: &AppHandle, thread_id: &str) -> String {
    use tauri::Manager;
    let state = app.state::<crate::AppStateHandle>();
    let s = state.lock().await;
    if let Some(subject_url) = s.thread_url_cache.get(thread_id) {
        crate::github::subject_url_to_html_url(subject_url)
    } else {
        "https://github.com/notifications".to_string()
    }
}

fn open_url(app: &AppHandle, url: &str) {
    if let Err(e) = app.opener().open_url(url, None::<&str>) {
        log::error!("ブラウザ起動失敗: {}", e);
    }
}
