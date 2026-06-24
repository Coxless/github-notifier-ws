use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn config_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(appdata)
            .join("github-notifier-ws")
            .join("config.yaml")
    }
    #[cfg(not(target_os = "windows"))]
    {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("github-notifier-ws")
            .join("config.yaml")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub poll_interval: u64,
    pub bundle_threshold: usize,
    pub allow_destructive: bool,
    pub default: Action,
    pub rules: Vec<Rule>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            poll_interval: 60,
            bundle_threshold: 3,
            allow_destructive: false,
            default: Action::Ignore,
            rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    #[serde(rename = "match")]
    pub conditions: Conditions,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Conditions {
    pub reason: Option<Vec<String>>,
    pub repository: Option<String>,
    pub subject_type: Option<String>,
    pub title_contains: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Notify,
    #[default]
    Ignore,
    MarkRead,
    MarkDone,
}

pub fn load(path: &Path) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("設定ファイルを読めませんでした: {}", path.display()))?;
    serde_yaml::from_str(&content).context("YAML のパースに失敗しました")
}

pub fn load_or_default() -> Config {
    let path = config_path();
    match load(&path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("設定ロード失敗、デフォルト使用: {}", e);
            Config::default()
        }
    }
}

const DEFAULT_CONFIG: &str = r#"# github-notifier-ws 設定ファイル
poll_interval: 60          # ポーリング間隔の下限（秒）。X-Poll-Interval が優先
bundle_threshold: 3        # 1回のポーリングでこの件数以上なら束ねて1枚に
allow_destructive: false   # mark_read / mark_done を実際に発火させるか
default: ignore            # どのルールにもマッチしない通知の扱い

rules:
  - name: direct
    match:
      reason: [mention, review_requested, assign]
    action: notify

  # - name: active-repo PRs
  #   match:
  #     repository: octocat/api-server
  #     subject_type: PullRequest
  #   action: notify

  # - name: ci-noise
  #   match:
  #     reason: ci_activity
  #   action: mark_read   # allow_destructive: true のときだけ実発火
"#;

pub fn append_ignore_rule(repo: &str) -> Result<()> {
    let path = config_path();
    let mut config = load(&path).unwrap_or_default();

    // Avoid duplicates
    let already_exists = config.rules.iter().any(|r| {
        r.action == Action::Ignore
            && r.conditions.repository.as_deref() == Some(repo)
            && r.conditions.reason.is_none()
            && r.conditions.subject_type.is_none()
            && r.conditions.title_contains.is_none()
    });
    if already_exists {
        return Ok(());
    }

    config.rules.push(Rule {
        name: format!("mute {}", repo),
        conditions: Conditions {
            repository: Some(repo.to_string()),
            ..Default::default()
        },
        action: Action::Ignore,
    });

    let yaml =
        serde_yaml::to_string(&config).context("設定のシリアライズに失敗")?;
    std::fs::write(&path, format!("# github-notifier-ws config\n{}", yaml))
        .context("ミュートルールの書き込みに失敗")
}

pub async fn start_watcher(app: tauri::AppHandle, state: crate::AppStateHandle) {
    use notify::{EventKind, RecursiveMode, Watcher};

    let path = config_path();

    // Ensure directory and default config exist
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if !path.exists() {
        let _ = std::fs::write(&path, DEFAULT_CONFIG);
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(16);

    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(evt) = res {
            if matches!(
                evt.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
            ) {
                let _ = tx.blocking_send(());
            }
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            log::error!("ファイル監視の初期化失敗: {}", e);
            return;
        }
    };

    if let Some(dir) = path.parent() {
        let _ = watcher.watch(dir, RecursiveMode::NonRecursive);
    }

    while rx.recv().await.is_some() {
        // Debounce
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        while rx.try_recv().is_ok() {}

        match load(&path) {
            Ok(new_config) => {
                let rule_count = new_config.rules.len();
                {
                    let mut s = state.lock().await;
                    s.config = new_config;
                }
                crate::notify::show_reload_success(&app, rule_count);
            }
            Err(e) => {
                crate::notify::show_config_error(&app, &e.to_string());
            }
        }
    }
}
