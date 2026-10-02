use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::auth::{now_seconds, Credentials};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const ACTIVITY_URL: &str = "https://chatgpt.com/backend-api/wham/profiles/me";

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
    pub percent: Option<f64>,
    pub resets_at: Option<i64>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub state: String,
    pub account_email: Option<String>,
    pub plan_label: Option<String>,
    pub tracks: Vec<TrackUsage>,
    pub error: Option<String>,
    pub activity_error: Option<String>,
    pub fetched_at: Option<i64>,
}

#[derive(Deserialize)]
struct UsageResponse {
    plan_type: String,
    email: Option<String>,
    rate_limit: Option<RateLimit>,
    code_review_rate_limit: Option<RateLimit>,
    additional_rate_limits: Option<Vec<AdditionalLimit>>,
    credits: Option<serde_json::Value>,
    spend_control: Option<serde_json::Value>,
    rate_limit_reset_credits: Option<serde_json::Value>,
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
            percent: Some(window.used_percent.clamp(0.0, 100.0)),
            resets_at,
            value: None,
        });
    }
}

// 크레딧 잔액과 횟수에는 전체 한도가 없으므로 사용률을 만들어 내지 않습니다.
fn append_value(tracks: &mut Vec<TrackUsage>, kind: &str, label: &str, value: String) {
    tracks.push(TrackUsage {
        label: label.into(),
        kind: kind.into(),
        percent: None,
        resets_at: None,
        value: Some(value),
    });
}

fn numeric_value(value: Option<&serde_json::Value>) -> Option<f64> {
    let value = value?;
    let number = value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse::<f64>().ok())?;
    (number.is_finite() && number >= 0.0).then_some(number)
}

fn message_range(value: Option<&serde_json::Value>) -> Option<String> {
    let range = value?.as_array()?;
    if range.len() != 2 {
        return None;
    }
    let min = numeric_value(range.first())?;
    let max = numeric_value(range.get(1))?;
    if min > max || min.fract() != 0.0 || max.fract() != 0.0 {
        return None;
    }
    Some(if min == max {
        format!("약 {min}회")
    } else {
        format!("약 {min}–{max}회")
    })
}

fn append_credit_tracks(tracks: &mut Vec<TrackUsage>, credits: &serde_json::Value) {
    if !credits.is_object() {
        return;
    }
    let unlimited = credits.get("unlimited").and_then(|v| v.as_bool()) == Some(true);
    let balance = numeric_value(credits.get("balance"));
    let value = if unlimited {
        "무제한".into()
    } else if let Some(balance) = balance {
        format!("{balance} 크레딧 남음")
    } else {
        match credits.get("has_credits").and_then(|v| v.as_bool()) {
            Some(false) => "크레딧 없음".into(),
            Some(true) => "잔액 정보 없음".into(),
            None => return,
        }
    };
    append_value(tracks, "credits:balance", "크레딧 잔액", value);
    if !unlimited {
        for (field, kind, label) in [
            (
                "approx_local_messages",
                "credits:local",
                "크레딧 · 로컬 작업",
            ),
            (
                "approx_cloud_messages",
                "credits:cloud",
                "크레딧 · 클라우드 작업",
            ),
        ] {
            if let Some(value) = message_range(credits.get(field)) {
                append_value(tracks, kind, label, value);
            }
        }
    }
}

fn append_reset_credits(tracks: &mut Vec<TrackUsage>, credits: &serde_json::Value) {
    let Some(available) = numeric_value(credits.get("available_count")) else {
        return;
    };
    if available.fract() != 0.0 {
        return;
    }
    let applicable = numeric_value(credits.get("applicable_available_count"))
        .filter(|count| count.fract() == 0.0 && *count <= available);
    let value = match applicable {
        Some(count) => format!("{available}회 · 사용 가능 {count}회"),
        None => format!("{available}회 남음"),
    };
    append_value(tracks, "credits:resets", "한도 초기화", value);
}

fn append_spend_limit(tracks: &mut Vec<TrackUsage>, control: &serde_json::Value, now: i64) {
    let Some(limit) = control.get("individual_limit").filter(|v| v.is_object()) else {
        return;
    };
    let amounts = numeric_value(limit.get("used")).zip(numeric_value(limit.get("limit")));
    let percent = numeric_value(limit.get("used_percent"))
        .or_else(|| numeric_value(limit.get("remaining_percent")).map(|p| 100.0 - p))
        .or_else(|| {
            amounts
                .filter(|(_, total)| *total > 0.0)
                .map(|(used, total)| used / total * 100.0)
        });
    let Some(percent) = percent.filter(|p| p.is_finite()) else {
        return;
    };
    let resets_at = limit
        .get("reset_at")
        .and_then(|v| v.as_i64())
        .filter(|t| *t > 0)
        .or_else(|| {
            limit
                .get("reset_after_seconds")
                .and_then(|v| v.as_i64())
                .filter(|s| *s >= 0)
                .and_then(|s| now.checked_add(s))
        });
    tracks.push(TrackUsage {
        label: "개인 사용 한도".into(),
        kind: "spend-control:individual".into(),
        percent: Some(percent.clamp(0.0, 100.0)),
        resets_at,
        value: amounts.map(|(used, total)| format!("{used} / {total} 크레딧 사용")),
    });
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
    if let Some(control) = response.spend_control {
        append_spend_limit(&mut tracks, &control, now);
    }
    if let Some(credits) = response.credits {
        append_credit_tracks(&mut tracks, &credits);
    }
    if let Some(credits) = response.rate_limit_reset_credits {
        append_reset_credits(&mut tracks, &credits);
    }
    Ok(UsageSnapshot {
        state: "OK".into(),
        account_email: response.email,
        plan_label: Some(plan_label(&response.plan_type)),
        tracks,
        error: None,
        activity_error: None,
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

fn fetch_json(
    credentials: &Credentials,
    url: &str,
    timeout: Duration,
) -> Result<String, UsageError> {
    // ureq의 기본 연결 제한은 전체 요청 제한보다 우선하므로 둘 다 명시합니다.
    let agent = ureq::AgentBuilder::new()
        .timeout(timeout)
        .timeout_connect(timeout)
        .redirects(0) // 인증 헤더를 다른 호스트로 전달하지 않습니다.
        .build();
    let response = agent
        .get(url)
        .timeout(timeout)
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
    response.into_string().map_err(|_| UsageError::Parse)
}

fn append_activity_result(snapshot: &mut UsageSnapshot, result: Result<String, UsageError>) {
    match result.and_then(|raw| crate::activity::parse_activity(&raw)) {
        Ok(tracks) => snapshot.tracks.extend(tracks),
        Err(error) => {
            snapshot.activity_error = Some(format!(
                "Codex 활동 통계를 불러오지 못했습니다. 기존 한도·크레딧 조회는 정상입니다. {error}"
            ));
        }
    }
}

pub fn fetch_usage(credentials: &Credentials) -> Result<UsageSnapshot, UsageError> {
    // 선택적 활동 조회가 기존 한도 조회를 지연시키거나 실패시키지 않도록 병렬 조회합니다.
    std::thread::scope(|scope| {
        let activity =
            scope.spawn(|| fetch_json(credentials, ACTIVITY_URL, Duration::from_secs(8)));
        let raw = fetch_json(credentials, USAGE_URL, Duration::from_secs(25))?;
        let mut snapshot = parse_usage(&raw, now_seconds())?;
        append_activity_result(
            &mut snapshot,
            activity.join().unwrap_or(Err(UsageError::Network)),
        );
        Ok(snapshot)
    })
}

pub fn error_snapshot(state: &str, message: String) -> UsageSnapshot {
    UsageSnapshot {
        state: state.into(),
        account_email: None,
        plan_label: None,
        tracks: Vec::new(),
        error: Some(message),
        activity_error: None,
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
            println!(
                "{}: percent={:?}, value={:?}",
                track.label, track.percent, track.value
            );
        }
        assert!(snapshot.activity_error.is_none(), "활동 통계 조회 실패");
    }

    #[test]
    fn plus_has_session_and_weekly_windows() {
        let snapshot = parse_usage(include_str!("../tests/fixtures/plus.json"), 100).unwrap();
        assert_eq!(snapshot.tracks.len(), 2);
        assert_eq!(snapshot.tracks[0].label, "5시간");
        assert_eq!(snapshot.tracks[0].percent, Some(32.0));
        assert_eq!(snapshot.tracks[0].resets_at, Some(18_100));
        assert_eq!(snapshot.tracks[1].label, "주간");
        assert_eq!(snapshot.plan_label.as_deref(), Some("Plus"));
    }

    #[test]
    fn weekly_primary_is_not_mislabeled_as_session() {
        let snapshot = parse_usage(include_str!("../tests/fixtures/prolite.json"), 100).unwrap();
        assert_eq!(snapshot.tracks.len(), 2);
        assert_eq!(snapshot.tracks[0].label, "주간");
        assert_eq!(snapshot.tracks[0].percent, Some(41.0));
        assert_eq!(snapshot.tracks[1].kind, "credits:balance");
        assert_eq!(snapshot.tracks[1].percent, None);
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
        assert_eq!(snapshot.tracks[0].percent, Some(100.0));
        assert_eq!(snapshot.tracks[0].resets_at, Some(180));
        assert_eq!(snapshot.plan_label.as_deref(), Some("future"));
    }

    #[test]
    fn credit_balances_estimates_and_reset_counts_have_no_fabricated_gauges() {
        let snapshot = parse_usage(include_str!("../tests/fixtures/credits.json"), 100).unwrap();
        assert_eq!(snapshot.tracks.len(), 6);
        let credits = &snapshot.tracks[2..];
        assert!(credits
            .iter()
            .all(|track| track.percent.is_none() && track.resets_at.is_none()));
        assert_eq!(credits[0].kind, "credits:balance");
        assert_eq!(credits[0].value.as_deref(), Some("125.5 크레딧 남음"));
        assert_eq!(credits[1].value.as_deref(), Some("약 25–100회"));
        assert_eq!(credits[2].value.as_deref(), Some("약 5–20회"));
        assert_eq!(credits[3].value.as_deref(), Some("3회 · 사용 가능 2회"));
        let spend = &snapshot.tracks[1];
        assert_eq!(spend.kind, "spend-control:individual");
        assert_eq!(spend.percent, Some(75.0));
        assert_eq!(spend.resets_at, Some(3700));
        assert_eq!(spend.value.as_deref(), Some("750 / 1000 크레딧 사용"));
    }

    #[test]
    fn unlimited_zero_and_unknown_credit_balances_are_distinct() {
        for (credits, expected) in [
            (
                r#"{"unlimited":true,"balance":"0","approx_local_messages":[0,0]}"#,
                "무제한",
            ),
            (r#"{"has_credits":false,"balance":0}"#, "0 크레딧 남음"),
            (r#"{"has_credits":false,"balance":null}"#, "크레딧 없음"),
            (r#"{"has_credits":true,"balance":null}"#, "잔액 정보 없음"),
        ] {
            let raw = format!(r#"{{"plan_type":"plus","credits":{credits}}}"#);
            let snapshot = parse_usage(&raw, 100).unwrap();
            assert_eq!(snapshot.tracks.len(), 1);
            assert_eq!(snapshot.tracks[0].value.as_deref(), Some(expected));
            assert_eq!(snapshot.tracks[0].percent, None);
        }
    }

    #[test]
    fn invalid_optional_metrics_do_not_break_existing_usage() {
        let snapshot = parse_usage(r#"{"plan_type":"plus",
            "rate_limit":{"primary_window":{"used_percent":20}},
            "credits":{"balance":"NaN","approx_local_messages":[10,5],"approx_cloud_messages":[-1,10]},
            "spend_control":{"individual_limit":{"used":"NaN","limit":"0"}},
            "rate_limit_reset_credits":{"available_count":-1}}"#, 100).unwrap();
        assert_eq!(snapshot.tracks.len(), 1);
        assert_eq!(snapshot.tracks[0].percent, Some(20.0));
    }

    #[test]
    fn spend_limit_uses_remaining_percentage_or_known_amounts() {
        for (limit, expected) in [
            (r#"{"remaining_percent":25,"reset_at":400}"#, 75.0),
            (r#"{"used":"32.5","limit":"50","reset_at":400}"#, 65.0),
            (r#"{"used_percent":120,"reset_at":400}"#, 100.0),
        ] {
            let raw = format!(
                r#"{{"plan_type":"business","spend_control":{{"individual_limit":{limit}}}}}"#
            );
            let snapshot = parse_usage(&raw, 100).unwrap();
            assert_eq!(snapshot.tracks.len(), 1);
            assert_eq!(snapshot.tracks[0].percent, Some(expected));
            assert_eq!(snapshot.tracks[0].resets_at, Some(400));
        }
    }

    #[test]
    fn unavailable_extras_are_omitted_and_zero_reset_counts_are_real() {
        let snapshot = parse_usage(
            r#"{"plan_type":"plus","credits":null,
            "spend_control":{"reached":false,"individual_limit":null},
            "rate_limit_reset_credits":{"available_count":0,"applicable_available_count":0}}"#,
            100,
        )
        .unwrap();
        assert_eq!(snapshot.tracks.len(), 1);
        assert_eq!(snapshot.tracks[0].kind, "credits:resets");
        assert_eq!(
            snapshot.tracks[0].value.as_deref(),
            Some("0회 · 사용 가능 0회")
        );
    }

    #[test]
    fn auth_permission_and_throttling_errors_are_distinct() {
        assert!(matches!(status_error(401), UsageError::Unauthorized));
        assert!(matches!(status_error(403), UsageError::Forbidden));
        assert!(matches!(status_error(429), UsageError::RateLimited));
        assert!(matches!(status_error(503), UsageError::Http(503)));
    }

    #[test]
    fn activity_is_merged_with_existing_limits_and_credits() {
        let mut snapshot =
            parse_usage(include_str!("../tests/fixtures/credits.json"), 100).unwrap();
        let original = serde_json::to_value(&snapshot.tracks).unwrap();
        append_activity_result(
            &mut snapshot,
            Ok(include_str!("../tests/fixtures/activity.json").into()),
        );
        assert_eq!(snapshot.tracks.len(), 12);
        assert_eq!(
            serde_json::to_value(&snapshot.tracks[..6]).unwrap(),
            original
        );
        assert!(snapshot.activity_error.is_none());
        assert_eq!(snapshot.fetched_at, Some(100));
    }

    #[test]
    fn optional_activity_failures_preserve_quota_and_report_a_separate_warning() {
        for result in [
            Err(UsageError::Network),
            Err(UsageError::Unauthorized),
            Err(UsageError::Forbidden),
            Err(UsageError::RateLimited),
            Ok("private-invalid-response".into()),
        ] {
            let mut snapshot =
                parse_usage(include_str!("../tests/fixtures/credits.json"), 100).unwrap();
            let original = serde_json::to_value(&snapshot.tracks).unwrap();
            append_activity_result(&mut snapshot, result);
            assert_eq!(serde_json::to_value(&snapshot.tracks).unwrap(), original);
            assert_eq!(snapshot.state, "OK");
            assert!(snapshot.error.is_none());
            let warning = snapshot.activity_error.unwrap();
            assert!(warning.contains("활동 통계"));
            assert!(!warning.contains("private-invalid-response"));
        }
    }
}
