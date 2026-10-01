mod auth;
mod install;
mod usage;

use auth::{default_credentials_path, read_credentials};
use install::{
    autostart_disable, autostart_enable, autostart_is_enabled, cleanup_stale_debug_autostart,
    ensure_installed_release, guard_debug_requires_vite,
};
use std::{sync::mpsc, time::Duration};
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};
use usage::{error_snapshot, fetch_usage, UsageError, UsageSnapshot};

const POLL_INTERVAL_MS: u64 = 300_000;
const SETTINGS_LABEL: &str = "settings";

/// 공식 사용량 페이지 한 곳만 엽니다. 토큰이나 계정 ID를 URL에 넣지 않습니다.
#[tauri::command]
fn open_chatgpt_usage() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("rundll32.exe")
        .args([
            "url.dll,FileProtocolHandler",
            "https://chatgpt.com/settings/usage",
        ])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW: 브라우저 실행 시 콘솔 숨김
        .spawn()
        .map(|_| ())
        .map_err(|_| {
            "ChatGPT 사용량 화면을 열지 못했습니다. 기본 브라우저 설정을 확인하세요.".into()
        })
}

#[tauri::command]
async fn get_usage() -> UsageSnapshot {
    tauri::async_runtime::spawn_blocking(read_usage)
        .await
        .unwrap_or_else(|_| {
            error_snapshot(
                "FetchError",
                "사용량 조회 작업을 완료하지 못했습니다.".into(),
            )
        })
}

fn read_usage() -> UsageSnapshot {
    let path = default_credentials_path();
    match read_credentials(&path) {
        Ok(cred) => match fetch_usage(&cred) {
            Ok(snap) => snap,
            // 만료 전 토큰이라도 서버가 거부할 수 있습니다. 그때도 갱신 실패가
            // 아니라 로그인 문제로 안내해야 사용자가 할 일이 분명해집니다.
            Err(UsageError::Unauthorized) => {
                error_snapshot("NeedLogin", UsageError::Unauthorized.to_string())
            }
            Err(e) => error_snapshot("FetchError", e.to_string()),
        },
        Err(e) => error_snapshot("NeedLogin", e.to_string()),
    }
}

#[tauri::command]
fn get_poll_interval_ms() -> u64 {
    POLL_INTERVAL_MS
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// 설정 창은 tauri.conf.json에 `visible: false`로 미리 선언해 두고 show/hide만
/// 토글한다. 클릭마다 새로 만들면 WebView2 초기화 지연이 그대로 노출된다.
/// 숨겨 뒀던 창을 다시 쓰는 것이라, 값을 새로 읽으라고 이벤트도 함께 보낸다.
#[tauri::command]
fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(SETTINGS_LABEL)
        .ok_or_else(|| "settings window not found".to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    window
        .emit("settings-open", ())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn close_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The preference lives in the webview, so the menu and startup both push it
/// onto the main window through here.
#[tauri::command]
fn set_always_on_top(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.set_always_on_top(enabled).map_err(|e| e.to_string())
}

#[tauri::command]
fn is_dev_build() -> bool {
    cfg!(debug_assertions)
}

#[tauri::command]
fn enable_autostart() -> Result<(), String> {
    autostart_enable()
}

#[tauri::command]
fn disable_autostart() -> Result<(), String> {
    autostart_disable()
}

#[tauri::command]
fn is_autostart_enabled() -> Result<bool, String> {
    autostart_is_enabled()
}

#[tauri::command]
fn install_release_copy() -> Result<String, String> {
    ensure_installed_release().map(|p| p.display().to_string())
}

/// 드래그가 멈춘 뒤 저장하여 재설치나 강제 종료에도 최근 위치를 유지합니다.
fn persist_position_on_move(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let (tx, rx) = mpsc::channel();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Moved(_)) {
            let _ = tx.send(());
        }
    });
    let handle = app.clone();
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
            // 플러그인의 cache lock과 창 getter가 서로 대기하지 않도록 메인 루프에서 저장합니다.
            let save_handle = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let _ = save_handle.save_window_state(StateFlags::POSITION);
            });
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    guard_debug_requires_vite();

    tauri::Builder::default()
        // 다른 플러그인보다 먼저 등록해야 두 번째 실행이 초기화 전에 끝납니다.
        // 작업 표시줄 버튼이 없어 가려진 위젯을 꺼낼 길이 재실행뿐이라,
        // 새로 띄우지 않고 기존 창을 앞으로 가져옵니다.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        // 위치만 복원합니다. 창 높이는 트랙 수에 따라 프런트가 정하므로,
        // 크기까지 저장하면 지난 실행의 트랙 수가 이번 실행을 덮어씁니다.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::POSITION)
                .with_denylist(&[SETTINGS_LABEL])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            get_usage,
            get_poll_interval_ms,
            quit_app,
            open_settings_window,
            close_settings_window,
            open_chatgpt_usage,
            is_dev_build,
            enable_autostart,
            disable_autostart,
            is_autostart_enabled,
            install_release_copy,
            set_always_on_top
        ])
        .on_window_event(|window, event| {
            if window.label() != SETTINGS_LABEL {
                return;
            }
            // 설정 창을 닫아도 앱은 살아 있어야 한다. 파괴하지 않고 숨긴다.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            persist_position_on_move(app.handle());
            cleanup_stale_debug_autostart();
            // Release builds keep a stable copy under LOCALAPPDATA for shortcuts/autostart.
            let _ = ensure_installed_release();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
