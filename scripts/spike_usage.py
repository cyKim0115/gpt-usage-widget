"""Codex 사용량 연결 확인. 토큰, 이메일, 계정 ID, 응답 원문은 출력하지 않습니다."""
from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import HTTPRedirectHandler, Request, build_opener


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def main() -> int:
    codex_dir = Path(os.environ.get("CODEX_HOME") or (Path.home() / ".codex"))
    try:
        auth = json.loads((codex_dir / "auth.json").read_text(encoding="utf-8"))
        if auth.get("auth_mode") == "apikey":
            print("ChatGPT 계정으로 Codex에 로그인하세요. API 키는 구독 한도를 제공하지 않습니다.")
            return 1
        tokens = auth.get("tokens") or {}
        token = tokens.get("access_token")
        account = tokens.get("account_id")
        if not token or not account:
            print("Codex 로그인 정보가 없습니다. codex login을 실행하세요.")
            return 1
        request = Request("https://chatgpt.com/backend-api/wham/usage", headers={
            "Authorization": f"Bearer {token}",
            "ChatGPT-Account-Id": account,
            "Accept": "application/json",
            "User-Agent": "gpt-usage-widget/0.1.0",
        })
        with build_opener(NoRedirect).open(request, timeout=25) as response:
            usage = json.load(response)
        output = {"plan": usage.get("plan_type"), "windows": []}
        limits = [("codex", usage.get("rate_limit")),
                  ("code-review", usage.get("code_review_rate_limit"))]
        limits.extend((entry.get("limit_name", "additional"), entry.get("rate_limit"))
                      for entry in usage.get("additional_rate_limits") or [])
        for name, limit in limits:
            for slot in ("primary_window", "secondary_window"):
                window = (limit or {}).get(slot)
                if window:
                    output["windows"].append({"name": name,
                        "seconds": window.get("limit_window_seconds"),
                        "usedPercent": window.get("used_percent"),
                        "resetsAt": window.get("reset_at")})
        print(json.dumps(output, ensure_ascii=False, indent=2))
        return 0
    except HTTPError as error:
        print(f"사용량 조회 실패: HTTP {error.code}")
    except URLError:
        print("사용량 서버 연결에 실패했습니다.")
    except (OSError, ValueError, TypeError, AttributeError):
        print("Codex 로그인 파일 또는 사용량 응답을 읽지 못했습니다.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
