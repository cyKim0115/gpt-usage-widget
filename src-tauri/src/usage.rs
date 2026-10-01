use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::auth::{now_seconds, Credentials};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

#[derive(Debug, Error)]
pub enum UsageError {
    #[error("Codex 로그인이 거부됐습니다. Codex에서 다시 로그인한 뒤 새로고침하세요.")]
    Unauthorized,
    #[error("사용량 조회 권한이 없습니다 (403). Codex의 로그인 계정과 워크스페이스를 확인하세요.")]
    Forbidden,
    #[error("조회 요청이 너무 많습니다 (429). 다음 자동 갱신을 기다려 주세요.")]
    RateLimited,
    #[error("사용량 서버 오류 (HTTP {0}). 다음 자동 갱신에서 다시 시도합니다.")]
    Http(u16),
    #[error("사용량 서버에 연결하지 못했습니다. 네트워크 연결을 확인하세요.")]
    Network,
    #[error("사용량 응답 형식이 변경됐거나 올바르지 않습니다.")]
    Parse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackUsage {
    pub label: String,
    pub kind: String,
    pub percent: f64,
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub state: String,
    pub account_email: Option<String>,
    pub plan_label: Option<String>,
    pub tracks: Vec<TrackUsage>,
    pub error: Option<String>,
    pub fetched_at: Option<i64>,
}

#[derive(Deserialize)]
struct UsageResponse {
    plan_type: String,
    email: Option<String>,
    rate_limit: Option<RateLimit>,
    code_review_rate_limit: Option<RateLimit>,
    additional_rate_limits: Option<Vec<AdditionalLimit>>,
}

#[derive(Deserialize)]
struct RateLimit {
    primary_window: Option<UsageWindow>,
    secondary_window: Option<UsageWindow>,
}

#[derive(Deserialize)]
struct UsageWindow {
    used_percent: f64,
    limit_window_seconds: Option<i64>,
    reset_at: Option<i64>,
    reset_after_seconds: Option<i64>,
}

#[derive(Deserialize)]
struct AdditionalLimit {
    limit_name: Option<String>,
    metered_feature: Option<String>,
    rate_limit: Option<RateLimit>,
}

fn window_label(seconds: Option<i64>) -> String {
    match seconds {
        Some(18_000) => "5시간".into(),
        Some(604_800) => "주간".into(),
        Some(s) if s > 0 && s % 86_400 == 0 => format!("{}일", s / 86_400),
        Some(s) if s > 0 && s % 3_600 == 0 => format!("{}시간", s / 3_600),
        Some(s) if s > 0 && s % 60 == 0 => format!("{}분", s / 60),
        Some(s) if s > 0 => format!("{s}초"),
        _ => "한도".into(),
    }
}

fn append_tracks(
    tracks: &mut Vec<TrackUsage>,
    limit: Option<RateLimit>,
    name: Option<&str>,
    kind: &str,
    now: i64,
) {
    let Some(limit) = limit else { return };
    // primary가 항상 5시간은 아닙니다. Pro Lite에서는 primary가 주간일 수 있습니다.
    for (slot, window) in [
        ("primary", limit.primary_window),
        ("secondary", limit.secondary_window),
    ] {
        let Some(window) = window else { continue };
        let duration = window_label(window.limit_window_seconds);
        let label = match name {
            Some(name) if !name.is_empty() => format!("{duration} · {name}"),
            _ => duration,
        };
        let resets_at = window.reset_at.filter(|t| *t > 0).or_else(|| {
            window
                .reset_after_seconds
                .filter(|s| *s >= 0)
                .and_then(|s| now.checked_add(s))
        });
        tracks.push(TrackUsage {
            label,
            kind: format!("{kind}:{slot}"),
            percent: window.used_percent.clamp(0.0, 100.0),
            resets_at,
        });
    }
}

fn plan_label(plan: &str) -> String {
    match plan {
        "free" => "Free",
        "go" => "Go",
        "plus" => "Plus",
        "pro" => "Pro",
        "prolite" | "pro_lite" => "Pro Lite",
        "team" => "Team",
        "business" => "Business",
        "enterprise" => "Enterprise",
        "edu" => "Edu",
        other => other,
    }
    .into()
}

pub fn parse_usage(raw: &str, now: i64) -> Result<UsageSnapshot, UsageError> {
    let response: UsageResponse = serde_json::from_str(raw).map_err(|_| UsageError::Parse)?;
    let mut tracks = Vec::new();
    append_tracks(&mut tracks, response.rate_limit, None, "codex", now);
    append_tracks(
        &mut tracks,
        response.code_review_rate_limit,
        Some("코드 리뷰"),
        "code-review",
        now,
    );
    for (index, limit) in response
        .additional_rate_limits
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        let kind = limit
            .metered_feature
            .unwrap_or_else(|| format!("additional-{index}"));
        let name = limit.limit_name.as_deref().unwrap_or(&kind);
        append_tracks(&mut tracks, limit.rate_limit, Some(name), &kind, now);
    }
    Ok(UsageSnapshot {
        state: "OK".into(),
        account_email: response.email,
        plan_label: Some(plan_label(&response.plan_type)),
        tracks,
        error: None,
        fetched_at: Some(now),
    })
}

fn status_error(status: u16) -> UsageError {
    match status {
        401 => UsageError::Unauthorized,
        403 => UsageError::Forbidden,
        429 => UsageError::RateLimited,
        other => UsageError::Http(other),
    }
}

pub fn fetch_usage(credentials: &Credentials) -> Result<UsageSnapshot, UsageError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(25))
        .redirects(0) // 인증 헤더를 다른 호스트로 전달하지 않습니다.
        .build();
    let response = agent
        .get(USAGE_URL)
        .set(
            "Authorization",
            &format!("Bearer {}", credentials.access_token),
        )
        .set("ChatGPT-Account-Id", &credentials.account_id)
        .set("Accept", "application/json")
        .set("User-Agent", "gpt-usage-widget/0.1.0")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(status, _) => status_error(status),
            ureq::Error::Transport(_) => UsageError::Network,
        })?;
    if response.status() != 200 {
        return Err(status_error(response.status()));
    }
    let raw = response.into_string().map_err(|_| UsageError::Parse)?;
    parse_usage(&raw, now_seconds())
}

pub fn error_snapshot(state: &str, message: String) -> UsageSnapshot {
    UsageSnapshot {
        state: state.into(),
        account_email: None,
        plan_label: None,
        tracks: Vec::new(),
        error: Some(message),
        fetched_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "현재 Codex 로그인과 네트워크가 필요한 수동 검증"]
    fn live_usage() {
        let credentials = crate::auth::read_credentials(&crate::auth::default_credentials_path())
            .expect("Codex 로그인 필요");
        let snapshot = fetch_usage(&credentials).expect("사용량 조회 실패");
        println!(
            "plan={:?}, tracks={}",
            snapshot.plan_label,
            snapshot.tracks.len()
        );
        for track in snapshot.tracks {
            println!("{}: {}%", track.label, track.percent);
        }
    }

    #[test]
    fn plus_has_session_and_weekly_windows() {
        let snapshot = parse_usage(include_str!("../tests/fixtures/plus.json"), 100).unwrap();
        assert_eq!(snapshot.tracks.len(), 2);
        assert_eq!(snapshot.tracks[0].label, "5시간");
        assert_eq!(snapshot.tracks[0].percent, 32.0);
        assert_eq!(snapshot.tracks[0].resets_at, Some(18_100));
        assert_eq!(snapshot.tracks[1].label, "주간");
        assert_eq!(snapshot.plan_label.as_deref(), Some("Plus"));
    }

    #[test]
    fn weekly_primary_is_not_mislabeled_as_session() {
        let snapshot = parse_usage(include_str!("../tests/fixtures/prolite.json"), 100).unwrap();
        assert_eq!(snapshot.tracks.len(), 1);
        assert_eq!(snapshot.tracks[0].label, "주간");
        assert_eq!(snapshot.tracks[0].percent, 41.0);
        assert_eq!(snapshot.plan_label.as_deref(), Some("Pro Lite"));
    }

    #[test]
    fn additional_and_review_limits_are_not_lost() {
        let snapshot = parse_usage(include_str!("../tests/fixtures/additional.json"), 100).unwrap();
        assert_eq!(snapshot.tracks.len(), 4);
        assert_eq!(snapshot.tracks[1].label, "주간 · 코드 리뷰");
        assert_eq!(snapshot.tracks[2].label, "5시간 · Extra model");
        assert_eq!(snapshot.tracks[3].label, "주간 · Extra model");
    }

    #[test]
    fn absent_windows_are_not_fabricated_as_zero_usage() {
        let snapshot = parse_usage(r#"{"plan_type":"enterprise","rate_limit":null}"#, 100).unwrap();
        assert!(snapshot.tracks.is_empty());
        assert!(parse_usage(
            r#"{"plan_type":"plus","rate_limit":{"primary_window":{"reset_at":200}}}"#,
            100
        )
        .is_err());
        assert!(parse_usage("{}", 100).is_err());
    }

    #[test]
    fn fallback_reset_and_unknown_duration_are_preserved() {
        let snapshot = parse_usage(r#"{"plan_type":"future","rate_limit":{"primary_window":{"used_percent":120,"limit_window_seconds":3600,"reset_after_seconds":80}}}"#, 100).unwrap();
        assert_eq!(snapshot.tracks[0].label, "1시간");
        assert_eq!(snapshot.tracks[0].percent, 100.0);
        assert_eq!(snapshot.tracks[0].resets_at, Some(180));
        assert_eq!(snapshot.plan_label.as_deref(), Some("future"));
    }

    #[test]
    fn auth_permission_and_throttling_errors_are_distinct() {
        assert!(matches!(status_error(401), UsageError::Unauthorized));
        assert!(matches!(status_error(403), UsageError::Forbidden));
        assert!(matches!(status_error(429), UsageError::RateLimited));
        assert!(matches!(status_error(503), UsageError::Http(503)));
    }
}
