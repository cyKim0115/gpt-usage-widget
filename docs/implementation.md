# 구현 기록

- Request type: out_of_scope (도구 앱 개발)
- Ceremony: quick; 게임 시스템용 4역할 승인 루프는 적용하지 않음
- Status: 구현·QA 완료 (자동 검사 및 사용자 화면 조작 테스트)
- 사용자 선택: Codex 사용량 (5시간·주간 한도)

## 참고 저장소

`../cursor-usage-widget`: Tauri 2 + TypeScript/Vite, 사용률 70/90% 색상, 창 위치 저장, 설정 창, 안정된 설치 경로의 자동 실행.

`../claude-usage-widget`: 위 구조에 동적 트랙, 단일 인스턴스, 로그인 계정 표시, 읽기 전용 인증, 설정 창과 위젯 사이의 사용량 이벤트가 추가되어 있어 기본 틀로 사용.

참고 저장소의 파일과 Git 설정은 변경하지 않음. 게임 시스템 문서의 실행 지침은 이 앱의 기능 요구로 가져오지 않음.

## 범위와 성공 조건

1. 로그인한 ChatGPT 계정의 Codex 한도가 반환되면 실제 기간과 사용률을 표시.
2. 계정에 한도가 하나만 있으면 그 한도만 표시. primary/secondary 순서로 기간을 추정하지 않음.
3. 로그인 실패·권한 오류·조회 제한 시 적절한 안내를 표시하고 다음 갱신을 수행.
4. 사용자가 위젯을 이동하거나 설정을 바꾸면 기존 위젯과 같은 방식으로 반영.
5. 릴리스 설치 경로와 시작프로그램 이름이 두 참고 앱과 충돌하지 않음.

ChatGPT 웹 대화 한도, API 결제 사용량, 토큰 갱신, 별도 OAuth 로그인 UI, OS keyring은 범위 밖.

## 구현 결정

- `auth.json`의 access token·account ID를 읽고 Codex가 사용하는 내부 사용량 엔드포인트로 요청.
- access token JWT의 `exp`는 만료 안내에만 사용. 최종 인증은 서버가 담당.
- `CODEX_HOME`을 지원. 토큰은 프런트엔드로 전달하거나 로그에 기록하지 않음.
- HTTP 응답은 필요한 필드만 읽고 원문 오류 본문을 노출하지 않음. 다른 호스트로 리디렉션하지 않음.
- `limit_window_seconds`로 한도 라벨을 결정. `reset_at`은 Unix 초 단위이며 상대 리셋 시간도 지원.
- 네트워크 작업은 Tauri의 blocking worker에 위임. 프런트엔드는 동시에 하나의 조회만 수행.
- 설정 창은 위젯의 스냅샷을 공유. 수동 조회가 끝날 때까지 새로고침 버튼을 비활성화.
- 앱 이름·설치 경로·식별자·환경설정 키·개발 포트를 분리. 녹색 아이콘 사용. 게이지는 참고 위젯과 같은 파랑/주황/빨강을 사용.

## 검증

- `npm run build`: TypeScript 검사 및 두 HTML 엔트리의 Vite 빌드 성공.
- `npm test`: 프런트엔드 표시 로직 3개 통과.
- `cargo test`: 인증·응답·자동 실행 경로 테스트 13개 통과. 네트워크 테스트 1개는 기본 실행에서 제외.
- 수동 `live_usage` 테스트와 `npm run spike`: 현재 Codex 계정의 주간 한도를 정상 조회. 토큰·이메일·계정 ID는 출력하지 않음.
- `cargo clippy --all-targets -- -D warnings`: 통과.
- Windows 릴리스 빌드, LOCALAPPDATA 설치, 바탕화면 바로가기 생성 성공. 설치 exe와 릴리스 exe 해시 일치.
- 재실행 전후 PID 동일: 이미 실행 중인 위젯을 유지하고 기존 창을 표시.
- 실제 앱의 접근성 트리에서 플랜·주간 사용률·리셋 잔여일·갱신 시각·진행률 표시기 확인. Shift+F10으로 설정/종료 메뉴 표시 확인.
- 초기 에이전트 검증에서는 화면 캡처 도구가 시간초과하고 사용자 Esc 입력으로 Computer Use가 종료되어 설정 토글·드래그 입력 검수를 마치지 못함.
- 2026-10-01 사용자 확인: "화면 조작이 필요한 테스트는 내가 완료했어." 화면 조작 테스트 완료로 반영. 이 결과는 사용자 수행·확인에 근거하며, 에이전트가 해당 테스트를 재실행하거나 화면 캡처를 확보한 것은 아님.

## QA 반영

- 자동 실행 경로에 따옴표를 적용하여 공백이 있는 사용자 폴더 지원.
- 드래그 종료 후 300ms debounce로 위치 저장. 플러그인 캐시 잠금과 창 getter의 교착을 막기 위해 메인 루프에 저장을 예약.
- 릴리스 exe가 같으면 재실행 때 강제 종료/재설치하지 않음.
- 재빌드 입력에 설정 HTML·lockfile·build.rs·아이콘 포함.
- 기본 Windows PowerShell에 Get-FileHash가 없는 환경에서도 동작하도록 .NET SHA256 사용.
- 위젯은 사용자 조작 창이므로 숨기지 않도록 Normal 상태로 실행.

## 추가 사용량·표시 선택 (2026-10-01)

- 크레딧 잔액, 크레딧으로 가능한 예상 로컬/클라우드 작업 횟수, 한도 초기화 횟수, 개인 사용 한도를 기존 응답에서 읽습니다. 새 엔드포인트나 별도 인증은 추가하지 않았습니다.
- 크레딧·개인 한도 필드는 OpenAI의 [CreditStatusDetails](https://github.com/openai/codex/blob/main/codex-rs/codex-backend-openapi-models/src/models/credit_status_details.rs), [SpendControlLimitDetails](https://github.com/openai/codex/blob/main/codex-rs/codex-backend-openapi-models/src/models/spend_control_limit_details.rs)와 대조했습니다. 초기화 횟수의 `available_count` / `applicable_available_count`는 현재 응답의 필드·타입을 민감정보 없이 확인했습니다.
- 잔액·횟수는 `percent: null`인 숫자 행으로 표시합니다. 총량이 없는 값에 0% 게이지를 만들지 않으며, 무제한·잔액 없음·잔액 미제공을 구분합니다. 예상 작업 횟수는 서버가 제공하는 범위입니다.
- 설정의 「표시할 사용량」에서 기본 한도와 추가 항목을 각각 선택합니다. 서버의 항목 ID를 `gpt-usage-hidden-tracks`에 저장하여 순서 변경·일시 미제공·재실행 뒤에도 선택을 유지합니다. 새 항목은 기본 표시합니다.
- Tauri 이벤트와 storage 이벤트는 마지막 스냅샷만 다시 그립니다. 토글로 사용량 API를 다시 호출하지 않으며, 창 높이도 보이는 행에 맞춥니다. 전부 숨기면 설정 안내를 표시합니다.
- 게이지 기본색은 두 참고 위젯의 `#5b9fd4`로 통일했습니다. 70/90% 경계와 주황/빨강은 Claude 위젯의 값을 그대로 사용합니다. 추가 옵션이 많은 설정 창은 세로 스크롤할 수 있습니다.

검증: 프런트엔드 테스트 7개, Rust 테스트 18개, Clippy 경고 검사 통과. 기본 제외된 live_usage도 별도 실행하여 실제 로그인 계정에서 주간 한도·크레딧 잔액·예상 작업 횟수·초기화 횟수 5개 행을 확인했습니다. 인증 정보와 응답 원문은 출력하지 않았습니다.

브라우저 검증은 실제 HTML/TypeScript와 가상 Tauri IPC를 사용했습니다. 가상 데이터 8개 행/4개 게이지, 계산된 색상, 항목별 토글, 창 사이 즉시 반영과 추가 조회 없음, 리로드 후 설정 유지, 전체 숨김에서 복구, 매분 갱신 시 숫자 유지, 설정 스크롤을 확인했습니다. 별도 읽기 전용 QA에서 찾은 토글 후 키보드 포커스 손실을 재현한 뒤, 항목 구성이 같으면 체크 상태만 갱신하여 수정하고 Space 입력 후 포커스 유지도 검증했습니다. 네이티브 WebView의 사용자 입력 검증과는 별개입니다. 임시 테스트 스크립트·캡처는 Git 제외 경로인 `.test-dist/`에 있습니다.

최종 소스로 릴리스 실행 파일을 다시 빌드·설치·재시작했습니다. 설치 파일과 릴리스 파일의 SHA256 일치 및 설치 경로에서 실행 중인 프로세스를 확인했습니다. 별도 읽기 전용 QA의 재검토에서도 추가 수정 사항은 없었습니다.

## 일반 ChatGPT 사용량 연동 확인 (2026-10-01)

- 사용자 요청에 따라 일반 ChatGPT 대화 사용량의 추가 연동 가능성을 확인했습니다.
- [공식 가격 안내](https://learn.chatgpt.com/docs/pricing)는 ChatGPT Work와 Codex가 한도·크레딧을 공유한다고 설명합니다. 기존 위젯의 한도는 Work에도 해당하므로 화면과 설정의 범위 표시를 바로잡았습니다.
- 현재 Codex 인증으로 `/backend-api/models`는 HTTP 200이지만 모델 목록 전체에서 잔여 메시지 수를 확인하지 못했습니다. `/backend-api/usage_limits`, `/backend-api/usage-limits`, `/backend-api/usage`는 HTTP 404였습니다. 이 결과는 확인한 경로와 현재 인증에 대한 것이며, 다른 인증이나 비공개 경로까지 없다는 뜻은 아닙니다.
- Codex 클라이언트의 `/wham/profiles/me` 계정 토큰 활동 조회도 확인했습니다. 이 프로필의 통계는 일반 ChatGPT 대화의 메시지 잔여량을 입증하지 않으므로 대화 한도로 표시하지 않았습니다. 인증·계정 식별자·응답 원문은 로그나 파일에 기록하지 않았습니다.
- [OpenAI의 사용량 관리 안내](https://developers.openai.com/cookbook/articles/sign-in-with-chatgpt)에 나온 `https://chatgpt.com/settings/usage`를 설정에서 여는 기능을 추가했습니다. 고정 URL만 기본 브라우저로 열고 인증 정보를 전달하지 않습니다. 브라우저에 로그인한 계정 기준입니다.
- 일반 ChatGPT 잔여 메시지·딥 리서치·이미지 생성의 사용 한도는 검증된 조회 응답을 확보하지 못했으므로 새 수치나 게이지를 만들지 않았습니다. 현재 확인된 범위와 제약은 README와 설정에 명시했습니다.

검증: 프런트엔드 테스트 7개, Rust 테스트 18개, Clippy, 화면 빌드 통과. 실제 HTML/TypeScript와 가상 IPC로 공유 한도 문구, 사용량 페이지 버튼의 명령 전달·성공/실패 안내·버튼 재활성화, 설정 창의 추가 사용량 조회 없음, 기존 표시 선택·색상·포커스·스크롤을 확인했습니다. 버튼의 브라우저 실행 자체는 이 가상 IPC 테스트의 검증 대상에 포함되지 않습니다.

## Codex 활동 통계 추가 (2026-10-02)

- Request type: out_of_scope (도구 앱 기능 추가), Ceremony: quick. 기존 위젯의 표시 구조를 확장했습니다.
- 현재 Codex 인증으로 `/backend-api/wham/profiles/me`의 활동 통계 조회가 HTTP 200으로 성공했습니다. 이번 확인에서 `/backend-api/models`는 HTTP 403이었으며, 일반 ChatGPT 대화의 잔여 메시지 수는 확보하지 못했습니다.
- 프로필 필드는 OpenAI의 [TokenUsageProfileStats](https://github.com/openai/codex/blob/main/codex-rs/backend-client/src/types.rs)와 [프로필 조회 경로](https://github.com/openai/codex/blob/main/codex-rs/backend-client/src/client.rs)에 대조했습니다. 누적·일일 최대·최근 일일 토큰, 현재·최장 연속 사용일, 최장 턴 시간을 `Codex · …` 숫자 행으로 추가했습니다. 일반 채팅 사용 한도나 메시지 수로 해석하지 않습니다.
- 최근 일일 토큰은 유효한 날짜의 최신 버킷을 선택하며, 서버의 집계 날짜를 함께 표시합니다. 응답 순서나 오늘 날짜를 가정하지 않습니다. 잘못된 날짜·음수·소수·미제공 필드는 생략하며, 실제 0은 유지합니다.
- 기존 표시 설정의 항목 ID를 재사용하여 각 활동 행의 표시 여부를 저장합니다. 한도 없는 활동 통계에는 게이지나 초기화 시간을 만들지 않습니다.
- 사용 한도와 활동 조회는 같은 읽기 전용 인증으로 병렬 수행합니다. 한도 조회는 25초, 활동 조회는 8초의 요청·연결 제한을 각각 명시합니다. 활동 조회 실패는 별도 `activityError`로 안내하고 기존 한도·크레딧을 계속 표시합니다. 다음 갱신에서 다시 조회합니다.
- 인증 정보·계정 식별자·응답 원문은 출력하거나 파일에 저장하지 않았습니다. 테스트 fixture와 브라우저 캡처는 가상 데이터입니다.

검증: 프런트엔드 테스트 7개, Rust 테스트 24개, Clippy, TypeScript/Vite 빌드 통과. 수동 live_usage 검증도 성공하여 현재 계정의 기존 5개 항목과 활동 통계 6개 항목, 총 11개 행을 확인했습니다.

실제 HTML/TypeScript와 가상 Tauri IPC를 사용한 브라우저 검증에서 14개 행과 기존 4개 게이지, 활동 숫자·집계 날짜 표시와 폭, 활동 항목의 키보드 토글·포커스 유지·창 사이 즉시 반영·재실행 후 보존·추가 조회 없음, 분 단위 갱신 후 숫자 유지, 전체 숨김 복구와 스크롤, 활동 조회만 실패한 경우와 복구를 확인했습니다. 검증 스크립트와 가상 데이터 캡처는 Git 제외 경로인 `.test-dist/`에 있습니다.

별도 읽기 전용 QA에서 확인한 HTTP 라이브러리의 기본 연결 제한 문제는 조회별 요청·연결 제한을 함께 설정하여 수정했습니다. Rust 테스트·Clippy·실제 조회를 다시 확인했으며, QA 재검토에서도 추가 문제는 없었습니다.

최종 소스로 릴리스 실행 파일을 다시 빌드·설치·재시작했습니다. 릴리스와 설치 파일의 SHA256 일치 및 설치 경로의 실행 프로세스를 확인했습니다.

## ChatGPT 웹 세션 스파이크 — no-go (2026-10-12)

- rag `docs/assets/2026-10-12-widget-dedicated-profile-login-plan.md` 5장 Phase 0입니다. 전용 Chrome 프로필로 로그인받은 웹 세션이 Codex 토큰으로 못 얻는 ChatGPT 사용 한도(대화 잔여량·딥 리서치·이미지 생성)를 주는지 확인했습니다.
- 스크립트는 Git 제외 경로 `.test-dist/spike_web_session.mjs`, 결과는 `.test-dist/spike-web-report.json`입니다. 경로·키 이름·타입·상태 코드만 남겼고 토큰·쿠키·응답 값·이메일은 출력하거나 저장하지 않았습니다.

### 로그인

- 프로필 `%LOCALAPPDATA%\GPTUsageWidget\login-profile`, Chrome 154.0.8037.98. 사용자가 이메일 코드로 직접 로그인했습니다.
- `--remote-debugging-port=0`을 켠 창에서는 `--enable-automation` 없이도 `navigator.webdriver`가 true였고, auth.openai.com의 이메일 코드 요청이 Cloudflare 403 챌린지로 막혔습니다.
- 포트 없이 띄운 창(계획 2.4-7 대체 흐름)에서는 로그인됐습니다. 세션 쿠키 `__Secure-next-auth.session-token.0`·`.1`은 session 쿠키가 아니며 만료가 약 90일 뒤입니다.

### 기록한 요청

로그인 뒤 포트를 켠 창에서 `/settings/usage`, `/`, `/#settings`, `/codex/settings/usage`를 열고 `/backend-api/` 28개 경로를 기록했습니다.

- 이름상 ChatGPT 사용 한도 후보는 `GET /backend-api/pageConfigs/usage_limits` 하나였습니다. 브라우저 안에서도 403 HTML이어서 응답 구조를 확인하지 못했습니다. 같은 창에서 `settings/is_adult`, `user_granular_consent`, `payments/payment_methods` 등 7개 경로도 403 HTML이었습니다.
- 200 JSON 응답에서 일반 대화 잔여 메시지·딥 리서치·이미지 생성 한도로 볼 필드는 없었습니다. `wham/usage`는 위젯이 쓰는 응답과 같은 구조입니다.
- 이번 기록에는 `conversation/init` 요청이 나타나지 않았습니다.

### 평문 HTTP 재호출

Node fetch, 리디렉션 끔, UA는 위젯과 같은 `gpt-usage-widget/0.1.0`입니다. 기존 Codex 토큰(`auth.json` Bearer + `ChatGPT-Account-Id`)으로 먼저 호출했습니다. 웹 토큰은 `/api/auth/session` 평문 호출이 막혀 브라우저가 보낸 요청 헤더에서 메모리로만 가져와 시험했습니다.

| 경로 | 브라우저 | Codex 토큰 | 웹 토큰 |
|---|---|---|---|
| `GET /backend-api/pageConfigs/usage_limits` | 403 HTML | 403 Cloudflare | 403 Cloudflare |
| `GET /backend-api/accounts/check/v4-2023-04-27` | 200 | 403 Cloudflare | 403 Cloudflare |
| `GET /backend-api/settings/user` | 200 | 403 Cloudflare | 403 Cloudflare |
| `GET /backend-api/me` | 200 | 200 | 200 |
| `GET /backend-api/wham/usage` | 200 | 200 | 200 |
| `GET /backend-api/wham/rate-limit-reset-credits` | 200 | 200 | 200 |
| `GET /backend-api/wham/usage/credit-usage-events` | 200 | 200 | 200 |
| `GET /backend-api/wham/usage/chatpass/apps` | 200 | 200 | 200 |
| `GET /backend-api/ps/plugins/installed` | 200 | 200 | 200 |
| `GET /backend-api/files/library/mounted/capabilities` | 200 | 200 | 200 |
| `GET /api/auth/session` (쿠키) | — | — | 403 Cloudflare |

### 판정

- no-go. 웹 토큰이 통과한 경로는 모두 Codex 토큰으로도 같은 결과였습니다. 웹 세션으로만 얻어지는 한도 데이터를 평문 HTTP로 가져오지 못했고, 쿠키로 웹 토큰을 받는 `/api/auth/session`도 평문 HTTP에서 Cloudflare에 막혔습니다. 전용 프로필 로그인과 "ChatGPT 웹" 소스는 추가하지 않습니다.
- `pageConfigs/usage_limits`는 디버깅 포트를 켠 브라우저 안에서도 막혀, 읽으려면 자동화 표시를 숨기는 방식이 필요합니다. 봇 탐지 회피에 해당하므로 시험하지 않았습니다.
- 재검토 조건: 한도 경로가 Cloudflare 없이 응답하게 되거나, Codex 클라이언트가 ChatGPT 대화 한도 조회를 제공할 때.

## Git

Git 작성자: 사용자가 현재 저장소에 설정한 `cyKim <76chanyoung@gmail.com>`. 사용자 요청에 따라 아직 게시하지 않은 기존 커밋의 작성자도 이 설정으로 갱신합니다. 공개 저장소는 `cyKim0115/gpt-usage-widget`이며, Git 설정 자체는 변경하지 않습니다.
