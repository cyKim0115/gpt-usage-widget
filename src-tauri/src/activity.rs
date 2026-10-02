use serde_json::Value;

use crate::usage::{TrackUsage, UsageError};

fn count(value: Option<&Value>) -> Option<i64> {
    value?.as_i64().filter(|n| *n >= 0)
}

fn grouped_number(number: i64) -> String {
    let digits = number.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

fn append(tracks: &mut Vec<TrackUsage>, kind: &str, label: &str, value: String) {
    tracks.push(TrackUsage {
        label: format!("Codex · {label}"),
        kind: format!("activity:{kind}"),
        percent: None,
        resets_at: None,
        value: Some(value),
    });
}

fn valid_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(i, byte)| i != 4 && i != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let year = date[..4].parse::<u32>().unwrap_or(0);
    let month = date[5..7].parse::<u32>().unwrap_or(0);
    let day = date[8..].parse::<u32>().unwrap_or(0);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    year > 0 && day > 0 && day <= max_day
}

fn duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = seconds % 3600 / 60;
    let seconds = seconds % 60;
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{}시간", grouped_number(hours)));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}분"));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{seconds}초"));
    }
    parts.join(" ")
}

// Codex 프로필의 활동 통계입니다. 일반 ChatGPT 메시지 잔여량이나 사용 한도가 아닙니다.
pub fn parse_activity(raw: &str) -> Result<Vec<TrackUsage>, UsageError> {
    let response: Value = serde_json::from_str(raw).map_err(|_| UsageError::Parse)?;
    let stats = response
        .get("stats")
        .filter(|stats| stats.is_object())
        .ok_or(UsageError::Parse)?;
    let mut tracks = Vec::new();
    for (field, label, unit) in [
        ("lifetime_tokens", "누적 토큰", "토큰"),
        ("peak_daily_tokens", "일일 최대 토큰", "토큰"),
        ("current_streak_days", "연속 사용", "일"),
        ("longest_streak_days", "최장 연속 사용", "일"),
    ] {
        if let Some(number) = count(stats.get(field)) {
            append(
                &mut tracks,
                field,
                label,
                format!(
                    "{}{unit}",
                    grouped_number(number),
                    unit = if unit == "토큰" { " 토큰" } else { "일" }
                ),
            );
        }
    }
    if let Some(seconds) = count(stats.get("longest_running_turn_sec")) {
        append(
            &mut tracks,
            "longest_running_turn_sec",
            "최장 턴 시간",
            duration(seconds),
        );
    }
    if let Some(buckets) = stats.get("daily_usage_buckets").and_then(Value::as_array) {
        let latest = buckets
            .iter()
            .filter_map(|bucket| {
                let date = bucket.get("start_date")?.as_str()?;
                let tokens = count(bucket.get("tokens"))?;
                valid_date(date).then_some((date, tokens))
            })
            .max_by_key(|(date, _)| *date);
        if let Some((date, tokens)) = latest {
            // 서버 날짜를 그대로 보여 줍니다. 최신 버킷이 오늘이 아닐 수도 있습니다.
            append(
                &mut tracks,
                "daily_tokens",
                "최근 일일 토큰",
                format!("{} · {date}", grouped_number(tokens)),
            );
        }
    }
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_metrics_are_counts_without_quota_gauges() {
        let tracks = parse_activity(include_str!("../tests/fixtures/activity.json")).unwrap();
        assert_eq!(tracks.len(), 6);
        assert!(tracks.iter().all(|track| {
            track.kind.starts_with("activity:")
                && track.label.starts_with("Codex · ")
                && track.percent.is_none()
                && track.resets_at.is_none()
        }));
        assert_eq!(tracks[0].value.as_deref(), Some("12,345,678 토큰"));
        assert_eq!(tracks[1].value.as_deref(), Some("1,234,567 토큰"));
        assert_eq!(tracks[2].value.as_deref(), Some("4일"));
        assert_eq!(tracks[3].value.as_deref(), Some("12일"));
        assert_eq!(tracks[4].value.as_deref(), Some("1시간 49분 19초"));
        assert_eq!(tracks[5].value.as_deref(), Some("1,234,567 · 2026-10-01"));
    }

    #[test]
    fn zero_is_real_but_missing_or_invalid_fields_are_omitted() {
        let tracks = parse_activity(
            r#"{"stats":{"lifetime_tokens":0,"peak_daily_tokens":null,
            "current_streak_days":-1,"longest_streak_days":1.5,
            "longest_running_turn_sec":"private-value","daily_usage_buckets":null}}"#,
        )
        .unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].value.as_deref(), Some("0 토큰"));
        assert!(parse_activity(r#"{"stats":{}}"#).unwrap().is_empty());
        for raw in ["{}", "broken-private-value", r#"{"stats":null}"#] {
            let error = parse_activity(raw).unwrap_err();
            assert!(!error.to_string().contains("private-value"));
        }
    }

    #[test]
    fn latest_valid_day_is_selected_without_assuming_bucket_order_or_today() {
        let tracks = parse_activity(
            r#"{"stats":{"daily_usage_buckets":[
            {"start_date":"2026-10-01","tokens":100},
            {"start_date":"2026-10-02","tokens":-1},
            {"start_date":"2026-02-30","tokens":900},
            {"start_date":"2026-9-30","tokens":800},
            {"start_date":"2026-09-30","tokens":200}]}}"#,
        )
        .unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].value.as_deref(), Some("100 · 2026-10-01"));
        assert!(valid_date("2024-02-29"));
        assert!(!valid_date("2026-02-29"));
        assert!(!valid_date("éé-10-01"));
        assert!(!valid_date("2026-00-01"));
    }

    #[test]
    fn durations_preserve_zero_and_exact_units() {
        assert_eq!(duration(0), "0초");
        assert_eq!(duration(60), "1분");
        assert_eq!(duration(3600), "1시간");
        assert_eq!(duration(3661), "1시간 1분 1초");
    }
}
