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
            handle_thread(app, &thread_id, &do_param).await;
        }
        Some("inbox") => handle_inbox(app, &do_param).await,
        other => log::warn!("unknown deep link host: {:?}", other),
    }
}

async fn handle_thread(app: &AppHandle, thread_id: &str, action: &str) {
    match action {
        "open" => {
            // Open the thread on GitHub; URL reconstruction is approximate —
            // for the exact URL we'd need the subject URL from cache.
            open_url(app, "https://github.com/notifications");
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
            // Muting: append an ignore rule to the config (best-effort)
            // In a full implementation this would do a surgical YAML edit
            log::info!("mute: {} (TODO: YAML 追記)", thread_id);
        }
        _ => log::warn!("unknown thread action: {}", action),
    }
}

async fn handle_inbox(app: &AppHandle, action: &str) {
    match action {
        "open" => open_url(app, "https://github.com/notifications"),
        "mark_all_read" => {
            let token = match crate::github::get_stored_token() {
                Ok(t) => t,
                Err(e) => {
                    log::error!("mark_all_read: トークン取得失敗: {}", e);
                    return;
                }
            };
            // PUT /notifications marks all as read
            let result = reqwest::Client::builder()
                .user_agent("github-notifier-ws/0.1.0")
                .build()
                .unwrap()
                .put("https://api.github.com/notifications")
                .bearer_auth(&token)
                .header("Content-Length", "0")
                .send()
                .await;
            match result {
                Err(e) => log::error!("mark_all_read 失敗: {}", e),
                Ok(_) => log::info!("すべて既読化完了"),
            }
        }
        _ => log::warn!("unknown inbox action: {}", action),
    }
}

fn open_url(app: &AppHandle, url: &str) {
    if let Err(e) = app.opener().open_url(url, None::<&str>) {
        log::error!("ブラウザ起動失敗: {}", e);
    }
}
