use crate::github::Notification;

// ── Windows-only: toast XML construction + WinRT dispatch ────────────────────

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::Notification;
    use windows::{
        core::HSTRING,
        Data::Xml::Dom::XmlDocument,
        UI::Notifications::{ToastNotification, ToastNotificationManager},
    };

    // Must match what the installer registers as AUMID
    const AUMID: &str = "dev.coxless.github-notifier-ws";

    pub fn show(xml: &str) {
        if let Err(e) = try_show(xml) {
            log::error!("トースト表示エラー: {}", e);
        }
    }

    fn try_show(xml: &str) -> windows::core::Result<()> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(xml))?;
        let notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        notifier.Show(&toast)?;
        Ok(())
    }

    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }

    fn reason_human(r: &str) -> &str {
        match r {
            "mention" => "あなたがメンションされました",
            "review_requested" => "レビュー依頼",
            "assign" => "あなたがアサインされました",
            "ci_activity" => "CI 結果",
            "team_mention" => "チームメンション",
            "state_change" => "状態変更",
            "security_alert" => "セキュリティアラート",
            "subscribed" => "サブスクリプション",
            _ => r,
        }
    }

    fn subject_type_human(t: &str) -> &str {
        match t {
            "PullRequest" => "プルリクエスト",
            "Issue" => "Issue",
            "Commit" => "コミット",
            "Release" => "リリース",
            "Discussion" => "ディスカッション",
            _ => t,
        }
    }

    /// Try to extract a line number from a serde_yaml error message.
    pub fn extract_line_number(msg: &str) -> Option<usize> {
        // serde_yaml errors: "... at line 5 column 3" or "line 5 column 3: ..."
        let idx = msg.find("line ")?;
        msg[idx + 5..]
            .split_whitespace()
            .next()?
            .trim_end_matches(',')
            .parse()
            .ok()
    }

    pub fn notification_xml(notif: &Notification) -> String {
        let id = notif.thread_id();
        let repo = escape(&notif.repository.full_name);
        let reason = escape(reason_human(&notif.reason));
        let stype = escape(subject_type_human(&notif.subject.subject_type));
        let title = escape(&notif.subject.title);
        let repo_enc = urlencoding::encode(&notif.repository.full_name);

        format!(
            r#"<toast activationType="protocol" launch="github-notifier-ws://thread/{id}?do=open" duration="long">
  <visual>
    <binding template="ToastGeneric">
      <text>{repo}</text>
      <text>{reason} · {stype}</text>
      <text>{title}</text>
    </binding>
  </visual>
  <actions>
    <action content="開く" arguments="github-notifier-ws://thread/{id}?do=open" activationType="protocol"/>
    <action content="既読にする" arguments="github-notifier-ws://thread/{id}?do=mark_read" activationType="protocol"/>
    <action content="このリポジトリをミュート" arguments="github-notifier-ws://thread/{id}?do=mute&amp;repo={repo_enc}" activationType="protocol"/>
  </actions>
</toast>"#
        )
    }

    pub fn bundle_xml(notifications: &[Notification]) -> String {
        let n = notifications.len();
        let mention = notifications.iter().filter(|n| n.reason == "mention").count();
        let review = notifications
            .iter()
            .filter(|n| n.reason == "review_requested")
            .count();
        let ci = notifications.iter().filter(|n| n.reason == "ci_activity").count();
        let other = n - mention - review - ci;

        let mut parts = Vec::new();
        if mention > 0 {
            parts.push(format!("メンション {mention}"));
        }
        if review > 0 {
            parts.push(format!("レビュー依頼 {review}"));
        }
        if ci > 0 {
            parts.push(format!("CI {ci}"));
        }
        if other > 0 {
            parts.push(format!("その他 {other}"));
        }
        let summary = escape(&parts.join("・"));

        format!(
            r#"<toast activationType="protocol" launch="github-notifier-ws://inbox?do=open">
  <visual>
    <binding template="ToastGeneric">
      <text>{n} 件の新しい通知</text>
      <text>{summary}</text>
    </binding>
  </visual>
  <actions>
    <action content="受信箱を開く" arguments="github-notifier-ws://inbox?do=open" activationType="protocol"/>
    <action content="すべて既読にする" arguments="github-notifier-ws://inbox?do=mark_all_read" activationType="protocol"/>
  </actions>
</toast>"#
        )
    }

    pub fn config_error_xml(msg: &str) -> String {
        let title = match extract_line_number(msg) {
            Some(line) => format!("設定エラー · {}行目", line),
            None => "設定エラー".to_string(),
        };
        let title = escape(&title);
        let msg = escape(msg);
        format!(
            r#"<toast>
  <visual>
    <binding template="ToastGeneric">
      <text>{title}</text>
      <text>{msg}</text>
      <text>直前の有効な設定で動作を継続中</text>
    </binding>
  </visual>
</toast>"#
        )
    }

    pub fn reload_success_xml(rule_count: usize) -> String {
        format!(
            r#"<toast duration="short">
  <visual>
    <binding template="ToastGeneric">
      <text>設定を更新しました</text>
      <text>{rule_count} 個のルールが有効です</text>
    </binding>
  </visual>
</toast>"#
        )
    }
}

// ── Public API (cross-platform; stubs on non-Windows) ─────────────────────────

pub fn show_notification(_app: &tauri::AppHandle, notif: &Notification) {
    #[cfg(target_os = "windows")]
    windows_impl::show(&windows_impl::notification_xml(notif));
    #[cfg(not(target_os = "windows"))]
    log::info!("[stub toast] {}", notif.subject.title);
}

pub fn show_bundle(_app: &tauri::AppHandle, notifications: &[Notification]) {
    #[cfg(target_os = "windows")]
    windows_impl::show(&windows_impl::bundle_xml(notifications));
    #[cfg(not(target_os = "windows"))]
    log::info!("[stub bundle toast] {} 件", notifications.len());
}

pub fn show_config_error(_app: &tauri::AppHandle, msg: &str) {
    #[cfg(target_os = "windows")]
    windows_impl::show(&windows_impl::config_error_xml(msg));
    #[cfg(not(target_os = "windows"))]
    log::warn!("[stub error toast] {}", msg);
}

pub fn show_reload_success(_app: &tauri::AppHandle, rule_count: usize) {
    #[cfg(target_os = "windows")]
    windows_impl::show(&windows_impl::reload_success_xml(rule_count));
    #[cfg(not(target_os = "windows"))]
    log::info!("[stub reload toast] {} ルール", rule_count);
}
