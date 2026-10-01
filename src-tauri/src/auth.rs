use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Codex 로그인 파일이 없습니다. Codex에서 ChatGPT 계정으로 로그인하세요.")]
    FileMissing,
    #[error("Codex 로그인 파일을 읽지 못했습니다. 파일 접근 권한을 확인하세요.")]
    Read,
    #[error("Codex 로그인 파일 형식이 올바르지 않습니다. Codex에서 다시 로그인하세요.")]
    Invalid,
    #[error("API 키 로그인은 구독 한도를 제공하지 않습니다. codex login으로 ChatGPT 계정에 로그인하세요.")]
    ApiKey,
    #[error("Codex 로그인 정보가 없습니다. codex login을 실행하세요.")]
    TokenMissing,
    #[error("Codex 로그인이 만료됐습니다. Codex를 실행해 갱신한 뒤 새로고침하세요.")]
    Expired,
}

// 자격증명이 Debug 출력이나 프런트엔드 직렬화에 포함되지 않도록 합니다.
pub struct Credentials {
    pub access_token: String,
    pub account_id: String,
}

#[derive(Deserialize)]
struct AuthFile {
    auth_mode: Option<String>,
    tokens: Option<Tokens>,
}

#[derive(Deserialize)]
struct Tokens {
    access_token: Option<String>,
    account_id: Option<String>,
}

pub fn default_credentials_path() -> PathBuf {
    credentials_path(
        std::env::var_os("CODEX_HOME").map(PathBuf::from),
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_default(),
    )
}

fn credentials_path(codex_home: Option<PathBuf>, user_home: PathBuf) -> PathBuf {
    codex_home
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| user_home.join(".codex"))
        .join("auth.json")
}

pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn nonempty_header(value: Option<String>) -> Result<String, AuthError> {
    let value = value
        .filter(|v| !v.trim().is_empty())
        .ok_or(AuthError::TokenMissing)?;
    if value.chars().any(char::is_control) {
        return Err(AuthError::Invalid);
    }
    Ok(value)
}

fn parse_credentials(raw: &str, now: i64) -> Result<Credentials, AuthError> {
    // 파싱 오류의 원문에는 자격증명 값이 섞일 수 있어 그대로 반환하지 않습니다.
    let parsed: AuthFile = serde_json::from_str(raw).map_err(|_| AuthError::Invalid)?;
    if parsed.auth_mode.as_deref() == Some("apikey") {
        return Err(AuthError::ApiKey);
    }
    let tokens = parsed.tokens.ok_or(AuthError::TokenMissing)?;
    let access_token = nonempty_header(tokens.access_token)?;
    let account_id = nonempty_header(tokens.account_id)?;

    // JWT 만료 시각은 빠른 안내에만 사용합니다. 인증의 최종 판단은 서버가 합니다.
    if let Some(claims) = access_token
        .split('.')
        .nth(1)
        .and_then(|part| URL_SAFE_NO_PAD.decode(part).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    {
        if claims
            .get("exp")
            .and_then(|v| v.as_i64())
            .is_some_and(|exp| exp <= now)
        {
            return Err(AuthError::Expired);
        }
    }
    Ok(Credentials {
        access_token,
        account_id,
    })
}

/// 파일은 읽기만 합니다. 토큰 갱신과 refresh token 회전은 Codex가 맡습니다.
pub fn read_credentials(path: &Path) -> Result<Credentials, AuthError> {
    let raw = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AuthError::FileMissing
        } else {
            AuthError::Read
        }
    })?;
    parse_credentials(&raw, now_seconds())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_home_and_empty_override() {
        assert_eq!(
            credentials_path(Some(PathBuf::from("custom")), PathBuf::from("user")),
            PathBuf::from("custom/auth.json")
        );
        assert_eq!(
            credentials_path(Some(PathBuf::new()), PathBuf::from("user")),
            PathBuf::from("user/.codex/auth.json")
        );
    }

    #[test]
    fn reads_chatgpt_tokens_without_exposing_other_credentials() {
        let parsed = parse_credentials(r#"{"auth_mode":"chatgpt","tokens":{"access_token":"test-access","account_id":"test-account","refresh_token":"secret-refresh"}}"#, 100).unwrap();
        assert_eq!(parsed.access_token, "test-access");
        assert_eq!(parsed.account_id, "test-account");
    }

    #[test]
    fn rejects_api_key_login() {
        assert!(matches!(
            parse_credentials(
                r#"{"auth_mode":"apikey","OPENAI_API_KEY":"test-secret"}"#,
                100
            ),
            Err(AuthError::ApiKey)
        ));
    }

    #[test]
    fn rejects_missing_account_and_header_injection() {
        assert!(matches!(
            parse_credentials(r#"{"tokens":{"access_token":"test"}}"#, 100),
            Err(AuthError::TokenMissing)
        ));
        assert!(matches!(
            parse_credentials(
                r#"{"tokens":{"access_token":"test\r\nHeader: injected","account_id":"test"}}"#,
                100
            ),
            Err(AuthError::Invalid)
        ));
    }

    #[test]
    fn detects_expired_jwt() {
        let jwt = format!(
            "header.{}.signature",
            URL_SAFE_NO_PAD.encode(r#"{"exp":100}"#)
        );
        let raw =
            serde_json::json!({"tokens":{"access_token":jwt,"account_id":"test"}}).to_string();
        assert!(parse_credentials(&raw, 99).is_ok());
        assert!(matches!(
            parse_credentials(&raw, 100),
            Err(AuthError::Expired)
        ));
    }

    #[test]
    fn malformed_credentials_do_not_echo_secrets() {
        let error = parse_credentials(r#"{"tokens":"private-secret"}"#, 100)
            .err()
            .unwrap();
        assert!(!error.to_string().contains("private-secret"));
    }
}
