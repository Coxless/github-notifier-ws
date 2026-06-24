use crate::config::{Action, Config, Conditions};
use crate::github::Notification;

pub fn apply(config: &Config, notif: &Notification) -> Action {
    for rule in &config.rules {
        if matches_conditions(&rule.conditions, notif) {
            return rule.action.clone();
        }
    }
    config.default.clone()
}

fn matches_conditions(cond: &Conditions, notif: &Notification) -> bool {
    if let Some(reasons) = &cond.reason {
        if !reasons.iter().any(|r| r == &notif.reason) {
            return false;
        }
    }
    if let Some(repo) = &cond.repository {
        if repo != &notif.repository.full_name {
            return false;
        }
    }
    if let Some(stype) = &cond.subject_type {
        if stype != &notif.subject.subject_type {
            return false;
        }
    }
    if let Some(contains) = &cond.title_contains {
        if !notif.subject.title.contains(contains.as_str()) {
            return false;
        }
    }
    true
}
