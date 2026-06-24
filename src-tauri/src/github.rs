use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Notification {
    pub id: String,
    pub reason: String,
    pub unread: bool,
    pub url: String,
    pub repository: Repository,
    pub subject: Subject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Repository {
    pub full_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Subject {
    pub title: String,
    #[serde(rename = "type")]
    pub subject_type: String,
    #[allow(dead_code)]
    pub url: String,
}

impl Notification {
    pub fn thread_id(&self) -> &str {
        self.url.split('/').last().unwrap_or(&self.id)
    }

    #[allow(dead_code)]
    pub fn html_url(&self) -> String {
        // api.github.com/repos/owner/repo/pulls/1 → github.com/owner/repo/pull/1
        self.subject
            .url
            .replace("https://api.github.com/repos/", "https://github.com/")
            .replace("/pulls/", "/pull/")
    }
}

pub struct GitHubClient {
    client: Client,
    token: String,
}

impl GitHubClient {
    pub fn new(token: String) -> Self {
        GitHubClient {
            client: Client::builder()
                .user_agent("github-notifier-ws/0.1.0")
                .build()
                .expect("HTTP クライアントの構築に失敗"),
            token,
        }
    }

    pub async fn fetch_notifications(
        &self,
        last_modified: Option<&str>,
    ) -> Result<Option<PollResult>> {
        let mut req = self
            .client
            .get("https://api.github.com/notifications")
            .bearer_auth(&self.token)
            .header("X-GitHub-Api-Version", "2022-11-28")
            .query(&[("all", "false"), ("participating", "false")]);

        if let Some(lm) = last_modified {
            req = req.header("If-Modified-Since", lm);
        }

        let resp = req.send().await.context("通知の取得に失敗")?;

        let rate_remaining = resp
            .headers()
            .get("X-RateLimit-Remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());

        let server_poll_interval: u64 = resp
            .headers()
            .get("X-Poll-Interval")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);

        if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(None);
        }

        let new_last_modified = resp
            .headers()
            .get("Last-Modified")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        resp.error_for_status_ref().context("GitHub API エラー")?;

        let notifications: Vec<Notification> =
            resp.json().await.context("通知レスポンスのパースに失敗")?;

        Ok(Some(PollResult {
            notifications,
            last_modified: new_last_modified,
            server_poll_interval,
            rate_remaining,
        }))
    }

    pub async fn mark_read(&self, thread_id: &str) -> Result<()> {
        self.client
            .patch(format!(
                "https://api.github.com/notifications/threads/{}",
                thread_id
            ))
            .bearer_auth(&self.token)
            .header("Content-Length", "0")
            .send()
            .await
            .context("既読化リクエスト失敗")?
            .error_for_status()
            .context("既読化 API エラー")?;
        Ok(())
    }

    pub async fn mark_done(&self, thread_id: &str) -> Result<()> {
        self.mark_read(thread_id).await?;
        self.client
            .delete(format!(
                "https://api.github.com/notifications/threads/{}/subscription",
                thread_id
            ))
            .bearer_auth(&self.token)
            .send()
            .await
            .context("サブスクリプション解除リクエスト失敗")?
            .error_for_status()
            .context("サブスクリプション解除 API エラー")?;
        Ok(())
    }
}

pub struct PollResult {
    pub notifications: Vec<Notification>,
    pub last_modified: Option<String>,
    pub server_poll_interval: u64,
    pub rate_remaining: Option<u32>,
}

// Token storage via OS keyring (Windows Credential Manager on Windows)
const KEYRING_SERVICE: &str = "github-notifier-ws";
const KEYRING_USER: &str = "github-pat";

pub fn get_stored_token() -> Result<String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .context("キーリング エントリの作成に失敗")?
        .get_password()
        .context("GitHub トークンが見つかりません。セットアップを完了してください。")
}

pub fn store_token(token: &str) -> Result<()> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .context("キーリング エントリの作成に失敗")?
        .set_password(token)
        .context("トークンの保存に失敗")
}

pub async fn poll_loop(app: tauri::AppHandle, state: crate::AppStateHandle) {
    loop {
        // Check snooze
        {
            let s = state.lock().await;
            if let Some(until) = s.snooze_until {
                if std::time::Instant::now() < until {
                    let wait = s.server_poll_interval.max(s.config.poll_interval);
                    drop(s);
                    tokio::time::sleep(tokio::time::Duration::from_secs(wait)).await;
                    continue;
                }
            }
        }

        let token = match get_stored_token() {
            Ok(t) => t,
            Err(e) => {
                log::warn!("トークン取得失敗: {}", e);
                crate::tray::set_error(&app, "認証エラー: トークンが設定されていません").await;
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                continue;
            }
        };

        let last_modified = {
            let s = state.lock().await;
            s.last_modified.clone()
        };

        let client = GitHubClient::new(token);
        match client.fetch_notifications(last_modified.as_deref()).await {
            Ok(Some(result)) => {
                let new_lm = result.last_modified.clone();
                let server_interval = result.server_poll_interval;
                let rate = result.rate_remaining;

                {
                    let mut s = state.lock().await;
                    s.last_modified = new_lm;
                    s.server_poll_interval = server_interval;
                    if let Some(r) = rate {
                        s.api_rate_remaining = Some(r);
                    }
                    s.last_sync = Some(std::time::Instant::now());
                }

                process_notifications(&app, &state, &client, result.notifications).await;
            }
            Ok(None) => {
                let mut s = state.lock().await;
                s.last_sync = Some(std::time::Instant::now());
            }
            Err(e) => {
                log::error!("ポーリングエラー: {}", e);
                crate::tray::set_error(&app, &e.to_string()).await;
            }
        }

        let sleep_secs = {
            let s = state.lock().await;
            s.server_poll_interval.max(s.config.poll_interval)
        };
        tokio::time::sleep(tokio::time::Duration::from_secs(sleep_secs)).await;
    }
}

async fn process_notifications(
    app: &tauri::AppHandle,
    state: &crate::AppStateHandle,
    client: &GitHubClient,
    notifications: Vec<Notification>,
) {
    let (allow_destructive, bundle_threshold) = {
        let s = state.lock().await;
        (s.config.allow_destructive, s.config.bundle_threshold)
    };

    let config = {
        let s = state.lock().await;
        s.config.clone()
    };

    let mut to_notify = Vec::new();

    for notif in notifications.iter().filter(|n| n.unread) {
        match crate::rules::apply(&config, notif) {
            crate::config::Action::Notify => to_notify.push(notif.clone()),
            crate::config::Action::MarkRead if allow_destructive => {
                if let Err(e) = client.mark_read(notif.thread_id()).await {
                    log::error!("既読化失敗 {}: {}", notif.thread_id(), e);
                }
            }
            crate::config::Action::MarkDone if allow_destructive => {
                if let Err(e) = client.mark_done(notif.thread_id()).await {
                    log::error!("mark_done 失敗 {}: {}", notif.thread_id(), e);
                }
            }
            // Destructive actions without allow_destructive: fall back to notify
            crate::config::Action::MarkRead | crate::config::Action::MarkDone => {
                to_notify.push(notif.clone())
            }
            crate::config::Action::Ignore => {}
        }
    }

    {
        let mut s = state.lock().await;
        s.unread_count = to_notify.len();
    }

    if to_notify.is_empty() {
        crate::tray::set_idle(app).await;
        return;
    }

    crate::tray::set_unread(app, to_notify.len()).await;

    if to_notify.len() >= bundle_threshold {
        crate::notify::show_bundle(app, &to_notify);
    } else {
        for n in &to_notify {
            crate::notify::show_notification(app, n);
        }
    }
}
