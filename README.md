# GPT Usage Widget

Windows 바탕화면에서 **ChatGPT 계정의 Codex 사용 한도**를 보는 플로팅 위젯입니다.
`cursor-usage-widget`의 사용률 표시 방식과 `claude-usage-widget`의 Tauri 창·설정·설치 구조를 바탕으로 만들었습니다.

**ChatGPT 웹 대화 사용량이나 OpenAI API 요금은 표시하지 않습니다.**

## 실행

1. Codex 앱 또는 CLI에서 ChatGPT 계정으로 로그인합니다. CLI는 `codex login`을 사용합니다.
2. 이 폴더의 **`시작.bat`** 을 실행합니다.
3. 처음 한 번은 빌드 후 `%LOCALAPPDATA%\GPTUsageWidget\gpt-usage-widget.exe`에 설치합니다. 바탕화면에 **GPT Usage Widget** 바로가기도 만듭니다.
4. 이후에는 바로가기나 `시작.bat`으로 실행합니다. 중복 실행은 기존 창을 앞으로 가져옵니다.

첫 빌드에는 Windows, Node.js, Rust, Visual Studio C++ Build Tools와 WebView2가 필요합니다. 설치된 릴리스 위젯은 Node.js·Rust·CLI 프로세스 없이 실행됩니다.

## 화면과 설정

- 한 줄은 서버가 반환한 한도 하나입니다. **5시간·주간 사용률**, 초기화까지 남은 시간, 계정 플랜을 표시합니다.
- `primary`라는 이유만으로 5시간이라고 표시하지 않습니다. **실제 기간**을 사용하므로 주간 한도만 제공되는 계정은 주간 한 줄만 나타납니다.
- 추가 모델 한도나 코드 리뷰 한도가 반환되면 별도 줄에 표시합니다. 없는 한도를 0%로 만들지 않습니다.
- **크레딧 잔액**, 크레딧으로 가능한 **예상 로컬·클라우드 작업 횟수**, **한도 초기화 횟수**, **개인 사용 한도**도 서버가 제공하면 표시합니다. 예상 작업 횟수는 서버의 추정 범위이며 실제 작업에 따라 달라집니다.
- **우클릭 → 설정 → 표시할 사용량**에서 각 항목을 개별로 켜거나 끌 수 있습니다. 선택은 즉시 적용되며 다음 실행에도 유지됩니다. 새로 제공되는 항목은 기본적으로 표시합니다.
- 바는 **소진율**입니다. 다른 위젯과 같은 파랑이 기본이며, 70%부터 주황, 90%부터 빨강입니다. 총량이 없는 크레딧 잔액·횟수에는 게이지를 표시하지 않습니다.
- 5분마다 자동 갱신하며, 남은 시간 표시는 매분 갱신합니다.
- 위젯을 드래그하면 이동하며, 위치는 다음 실행에도 복원합니다.
- **우클릭 → 설정**에서 계정·플랜·사용률 확인, 수동 새로고침, 항상 위에 표시, Windows 자동 실행을 설정합니다.
- **우클릭 → 종료**로 앱을 종료합니다. 설정 창의 닫기와 `Esc`는 설정 창만 숨깁니다.
- 개발 모드에서는 Windows 자동 실행을 변경할 수 없습니다. 자동 실행은 설치된 릴리스 경로를 사용합니다.

## 인증과 오류

인증은 `%USERPROFILE%\.codex\auth.json`을 읽습니다. `CODEX_HOME` 환경 변수가 있으면 그 폴더의 `auth.json`을 사용합니다.
사용량은 `GET https://chatgpt.com/backend-api/wham/usage`에 로컬 access token과 계정 ID를 보내 조회합니다.

- 로그인 파일은 **읽기 전용**입니다. 위젯은 토큰을 저장·갱신하거나 refresh token을 사용하지 않습니다.
- 토큰이 만료되면 Codex를 실행해 로그인을 갱신하고 위젯 설정에서 새로고침합니다.
- API 키 로그인은 지원하지 않습니다. ChatGPT 계정으로 Codex에 로그인해야 합니다.
- OS 자격증명 저장소만 사용하는 `keyring` 로그인은 현재 지원하지 않습니다. 파일 기반 로그인이 필요합니다.
- `401`은 로그인 필요, `403`은 계정/워크스페이스 권한 오류, `429`는 일시적인 조회 제한으로 안내합니다.
- 인증 정보와 서버 응답 원문은 로그·화면에 출력하지 않습니다. 이메일은 설정 창에만 표시합니다.
- 조회는 최대 25초로 제한합니다. 오류가 나면 다음 자동 갱신에서 다시 시도합니다.

내부 사용량 HTTP 엔드포인트는 **비공식**이며 변경될 수 있습니다. 공식 Codex 사용량 화면을 기준으로 확인하세요.
OpenAI의 [Codex 인증 문서](https://learn.chatgpt.com/docs/auth)와 [사용 한도 필드 설명](https://learn.chatgpt.com/docs/app-server#6-rate-limits-chatgpt),
[Codex 클라이언트 구현](https://github.com/openai/codex/blob/main/codex-rs/backend-client/src/client.rs)을 참고했습니다.

## 개발 및 검증

```powershell
npm ci
npm run tauri dev
```

개발 서버는 `1424` 포트를 사용하여 다른 두 위젯의 개발 서버와 충돌하지 않습니다.
`target\debug` 실행 파일만 단독으로 실행하면 Vite가 없어 안내 후 종료합니다.

| 명령 | 용도 |
|---|---|
| `npm run build` | TypeScript 검사 및 화면 빌드 |
| `npm test` | 리셋 시간·갱신 시각·색상 경계·표시 설정 저장 검증 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 인증·응답 파싱·한도 기간·오류 분류 검증 |
| `npm run spike` | 현재 로그인으로 연결 확인, 민감정보 제외 |
| `npm run build:app` | 릴리스 실행 파일 빌드 (설치 패키지 생성 생략) |
| `npm run start:app` | 필요 시 빌드·설치 후 실행 |
| `npm run rebuild:app` | 강제 재빌드·설치·재시작 |
| `npm run make:icon` | 아이콘 재생성, Python Pillow 필요 |

Windows Rust 명령은 Visual Studio 개발 환경이 필요할 수 있습니다. `시작.bat`은 해당 환경을 자동으로 불러옵니다.
테스트 JSON은 가상 데이터이며 인증 파일은 저장소에 포함하지 않습니다.

설계·검증 기록: [docs/implementation.md](docs/implementation.md).
