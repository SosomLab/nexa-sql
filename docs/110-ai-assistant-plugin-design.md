# 110 — AI 어시스턴트 플러그인(Claude · Codex) 설계 초안

> 사용자 요구(2026-11-10 밤): "AI와 상호 소통하는 플러그인 — 좌측 레이아웃의 확장 기능 아래에 AI별 버튼 · 누르면 요청/응답 세션 · 모델 선택 ·
> 에이전트 표시 · `/` 명령 · Auto 모드 · 클립보드 붙여넣기 · 파일 첨부 · 토큰 표시 · 인증은 VS Code 확장처럼" + "**요청하면 이 앱의 모든
> 기능(쿼리 구성 · 테이블 검색 · 쿼리 포맷 · 워드랩 설정 변경 · 명령 팔레트 명령 …)을 API로 쓸 수 있게**" · 1차 = Claude · Codex 둘.
> 설계만(구현은 사용자 지시 뒤). 조사 = 공식 문서(Claude API 인증 · Claude Code headless/Agent SDK · Codex app-server/MCP) + 이 저장소
> (확장 체계 · 네트워크 · 볼트 · UI 부품 · 활동 막대). 결정 후보는 §10(D-번호는 docs/10에 협업이 등재).

## 0. 결론 먼저

| 질문 | 답 |
|---|---|
| VS Code 확장 수준(스트리밍 응답 · 도구 호출 표시 · 승인 · 토큰 · 모델/에이전트/`/`명령/Auto)이 가능한가 | **가능.** VS Code의 Claude Code 확장과 Codex 확장이 실제로 쓰는 방식 = **CLI 실행 파일을 자식 프로세스로 띄우고 구조화된 JSON 스트림으로 주고받기**(Claude `claude -p --output-format stream-json --input-format stream-json` · Codex `codex app-server` = JSONL JSON-RPC). 터미널 글자를 긁는 것이 아니라 이벤트(글자 조각 · 도구 호출 · 승인 요청 · 토큰 사용량 · 세션 ID)를 받아 UI를 그린다. 이 앱도 같은 길을 쓸 수 있다(`nsql_ext::net::quiet_command`로 콘솔 창 없이 spawn + stdin/stdout 파이프 + 수거 스레드 + `Wake`). |
| "앱의 모든 기능을 AI가 API로" | **MCP 서버**로 푼다. 두 에이전트 모두 MCP(stdio) 서버를 붙일 수 있다(Claude `--mcp-config` · Codex `mcp_servers`). 이 앱이 **자기 기능을 MCP 도구로 노출**(명령 팔레트 명령 실행 · 세션 질의 · 테이블/객체 검색 · 포맷 · 설정 읽기/쓰기 · 편집기 본문 읽기/넣기 · 결과 그리드 읽기)하면 에이전트가 자연어 요청을 도구 호출로 수행한다. 플러그인 자체 ABI(WASM v1)는 네트워크·패널·클립보드·스트리밍이 없어 **이 용도에는 못 쓴다** → 내장(in-process Rust) 확장 + 호스트 패널/뷰 + MCP 브리지. |
| 웹에서 토큰을 받는 인증(VS Code처럼) | **Claude: 제3자 제품이 claude.ai 로그인/구독 한도를 제공하는 것은 Anthropic이 허용하지 않는다**(Agent SDK 문서 명시 · "previously approved" 제외). 공식 = **API 키**(콘솔 발급 · 볼트에 봉투 저장 · 자식 프로세스에 `ANTHROPIC_API_KEY` 환경으로 전달 · `--bare`). 사용자가 **자기 PC에 설치한 Claude Code에 스스로 로그인**해 둔 것을 비-bare 모드가 읽는 것은 "사용자 개인 설정"이지만 제품 기능으로 **제공·안내하지 않는다**(약관 위험 · §4). **Codex**: 공식 "Sign in with ChatGPT"는 선정 파트너 시범(클라이언트 ID 등록) · Codex CLI의 OAuth(PKCE · localhost:1455)를 제3자가 재사용하는 것은 미문서. 공식 = **API 키**(`OPENAI_API_KEY`) · `codex app-server`가 **자기 로그인 상태**(사용자가 `codex login`으로 한 것)를 쓰는 것은 CLI 몫. 결론: **1차 인증 = API 키(둘 다) + 설치된 CLI의 자체 로그인 상태는 "있으면 그대로 동작"으로만**(앱은 OAuth를 구현하지 않음 · 토큰 파일을 읽거나 복사하지 않음). |
| 직접 API 호출(CLI 없이) 대안 | 가능하지만 범위가 커진다: 자체 TLS(`nsql-net` · chunked 수신 → SSE 파서 작성) · 도구 루프 · 권한 · 세션 · 서브에이전트 · `/` 명령을 **전부 직접** 만들어야 한다. CLI 호스팅은 그 모두를 CLI가 제공한다. **추천 = CLI 호스팅(1차)** · 직접 API는 "CLI가 없는 환경 · 단순 질의(SQL 설명·주석 생성)용 경량 모드"로 2차 후보. |
| 프로필 접속 · 질의 실행 · 비밀 | **플러그인/AI는 요청만 · 접속·실행·비밀은 호스트**(§7) — 기존 불변식(DR-22 볼트 · DR-34 `gate_pass`/`Sess::submit` · 61 §1-7 · T-132 가림)이 그대로 적용된다. |

## 1. 요구 → 기능 지도

| 요구(사용자) | 1차 범위 | 근거·방법 |
|---|---|---|
| 좌측 확장 기능 아래 AI별 버튼 | Claude · Codex 활동 막대 항목 2개(`view.ai_claude` · `view.ai_codex` · 확장 항목 뒤 · 기본 보임은 설정) | `activity.rs:78-130` 표에 `ActItem` 추가 · 클릭 = `menu_action` → 패널 토글(`view.extensions` 분기 본뜸) · `side_panel_close_others` · `Focus` 변형 · 팔레트 `commands::PALETTE` · 프로젝트 복원 표 |
| 요청/응답 세션 | 좌측 패널 = 세션 목록(새 세션 · 이어 하기 · 이름) + 상태(모델 · 모드 · 토큰) · 편집기 자리 **뷰 탭**(`ai:<provider>:<session>`) = 대화 본문(스트리밍) · 입력 상자 · 첨부 칩 · 보내기/중단 | `Editors::open_view_tab` · `ext_view` 선례(읽기 전용 TextBox 본문) · 입력 = multiline TextBox · 중단 = SIGINT/`turn/interrupt` |
| 모델 선택 | 패널 콤보. Claude = `/model <id>`(stream-json 입력으로 전달) 또는 `--model` · 목록 = 설정 표(`claude-opus-5-5` 기본 · sonnet-5-5 · haiku-5-5 · fable-5-1) · Codex = `model/list`(`supportedReasoningEfforts` 포함) | Claude `claude -p` 메모 "`/model`·`/effort`는 인자 허용" · Codex `thread/start.model` · `turn/start.effort` |
| 에이전트 표시 | Claude = `system/init`의 tools·mcp_servers·plugins · 서브에이전트 메시지(`parent_tool_use_id`) 트리 표시 · Codex = `item/started`/`item/completed` 항목 종류별 표시 | 둘 다 이벤트에 들어 있다 |
| `/` 명령 | 입력 상자 `/` = 팝업(앱 명령 `/new` `/model` `/effort` `/auto` `/attach` `/clear` `/sql` `/schema` `/help` + Claude 사용자 스킬 `/<skill>` 통과) | Claude는 `/skill-name`을 프롬프트에 넣으면 확장 · Codex는 앱이 해석 |
| Auto 모드 | 토글 = 권한 방식: Claude `--permission-mode auto`(분류기 심사) / `acceptEdits` / 수동(`--permission-prompt-tool` = 우리 MCP 승인 도구 → 앱 확인 창) · Codex `approvalPolicy` + `sandboxPolicy`(readOnly/workspaceWrite) · **DB 쓰기 질의는 모드와 무관하게 항상 확인**(§7) | 공식 플래그 그대로 |
| 토큰 표시 | Claude `result` 메시지의 usage·`total_cost_usd`(추정치) · stream_event 누적 · Codex 토큰 사용량 알림(이름 확인 필요 · 문서 뒷부분) | 패널 상태줄 + 세션별 누계 |
| 클립보드 붙여넣기 | 텍스트 = 있음(`clipboard.rs read_text`) · **이미지 = 없음 → nexa-ctl에 CF_DIB/PNG 읽기 추가**(2차) | 붙여넣은 이미지 = 첨부 칩 |
| 파일 첨부 | `nexa-dlg::FilePicker`(네이티브 0) + 창 드롭(`DroppedFile`) · 이미지 = Claude 입력 `image`(base64) / Codex `localImage` · 텍스트/SQL = 본문 삽입 · PDF = Claude `document` 블록(≤32 MB) · Codex는 경로 전달 | 크기 상한 설정 |
| 인증 | 설정 ▸ AI ▸ "API 키 설정…" → 볼트 봉투(새 `Vault::put_secret/get_secret` · ChaCha20-Poly1305 + 기기 키 DPAPI) · 자식 프로세스 환경으로만 전달 · 화면·로그에 길이만 | §4 |
| 앱 기능을 API로 | **MCP 서버**(§3) — 명령 팔레트 명령 전부 + 질의/검색/포맷/설정/편집기/그리드 도구 | 둘 다 MCP 지원 |

## 2. 아키텍처(4계층 · DR-7)

```
[UI]  활동 막대 항목 ─ AI 패널(세션·모델·모드·토큰) ─ 뷰 탭(대화 본문·입력·첨부·승인 카드)
        │ AiEvent(텍스트 조각·도구 호출·승인 요청·사용량·끝)        ▲ AiCmd(보내기·중단·모델·모드·첨부)
[Core] ai::Session(대화 모델 · 가림 필터 · `/` 명령 파서 · 이력 저장 NSQL_HOME/ai/) · ai::Provider 포트(trait) + 레지스트리
        │                                                            ▲
[Net]  ai::host::CliHost(자식 프로세스 · stdin/stdout JSONL · 수거 스레드 · Wake) · ai::mcp::Server(localhost 리스너 · 토큰 · JSON-RPC)
        │                                                            ▲
[Adapter] ClaudeCli(claude -p stream-json 프로토콜) · CodexAppServer(codex app-server JSON-RPC) · (2차) ClaudeApi(nsql-net + SSE)
```

- **포트(trait) + 레지스트리 + 설정 선택**(docs/30 §1): `trait AiProvider { fn id(); fn start(&self, cfg) -> Box<dyn AiHost>; fn models(); fn capabilities() }` · `AiHost { send_turn(input) · interrupt() · set_model/effort/mode() · events() }` · 레지스트리 `ai::PROVIDERS = [claude, codex]` · 설정 `ai.default_provider`.
- **CLI 호스팅 공통**: `quiet_command(exe)`(Windows CREATE_NO_WINDOW · stdin 파이프 · 61 §2-2 콘솔 창 금지) · stdout 줄 단위 수거 스레드 → mpsc → `proxy.send_event(Wake)` → 틱에서 drain(확장 저장소·GitWatch 선례) · stderr는 로그 창 · 종료 = SIGINT 뒤 SIGTERM(Claude: 143) · 자식 PID는 앱 종료 때 함께 정리.
- **실행 파일 탐색**: 설정 `ai.claude.exe` / `ai.codex.exe`(빈 값 = PATH에서 `claude`/`codex` · Windows는 `.cmd` 셰임 주의 · 없으면 패널에 "설치 안내" 카드 · 버전 = `claude --version`/`codex --version` 1회).
- **네트워크 부하 규칙(26 §8)**: 네트워크는 자식 CLI가 함(앱은 직접 소켓 0) · MCP 리스너는 localhost 전용 · 자동 재접속 없음 · 동시 세션 상한 `ai.max_sessions`(기본 2) · 39 §3 원장 등재(스레드 = 세션당 수거 1 · 끄기 = `ai.enabled`).

## 3. MCP 서버 — "앱 기능 = 도구"

### 3-1 형태
- 에이전트 CLI는 MCP **stdio** 서버를 자식으로 띄운다. GUI 프로세스 자체를 띄울 수는 없으므로 **브리지 실행 파일**을 둔다: `nsql mcp-bridge --port <p> --token <t>`(기존 CLI `nsql`에 서브커맨드) = stdin/stdout JSON-RPC를 GUI의 localhost 리스너(`127.0.0.1:0` 자동 포트 · 세션당 1회성 토큰)로 그대로 중계. GUI 쪽 `ai::mcp::Server`가 `initialize`/`tools/list`/`tools/call`을 처리한다(JSON = 저장소의 자체 파서 · 외부 crate 0).
- Claude: `--mcp-config '{"mcpServers":{"nexa":{"command":"<nsql>","args":["mcp-bridge","--port","…","--token","…"]}}}'` + `--bare`(사용자 홈의 훅·MCP 무시 = 재현성 · 보안) · Codex: 프로젝트 `.codex/config.toml` 대신 **app-server 실험 API `dynamicTools`**(스레드 시작 때 도구 목록 전달 · `toolOutput`으로 결과 회신)를 1순위로 검토 · 안 되면 `codex mcp add nexa -- <nsql> mcp-bridge …`를 앱이 사용자 홈 설정에 쓰는 것은 **사용자 확인 뒤**(홈 밖 리소스 수정 = 확인 대상 · 61 §2-4).
- 승인: Claude `--permission-prompt-tool mcp__nexa__approve`(권한 요청이 우리 도구로 옴 → 앱 확인 창 → allow/deny 회신) · Codex `item/permissions/requestApproval` 서버 요청에 앱이 응답.

### 3-2 도구 목록(1차 · 이름 = `nexa.<동사>_<대상>` · 전부 `strict` 스키마 · 읽기/쓰기 등급 표기)
| 도구 | 등급 | 하는 일 | 호스트 경로 |
|---|---|---|---|
| `list_commands` · `run_command(id, args?)` | 쓰기(명령별) | 명령 팔레트 명령 전부(`commands::PALETTE` + 키맵 `COMMANDS` · 토글 상태 포함) · 쓰기 등급 명령(파일 저장·실행·접속 해제)은 승인 | `menu_action(id)` |
| `get_settings(prefix?)` · `set_setting(key, value)` | 쓰기 | 설정 레지스트리 읽기/쓰기(라벨·설명·종류·허용값 포함 → "워드랩 켜 줘" = `set_setting("editor.word_wrap","on")`) | `settings.set` + `apply_setting` |
| `get_editor(tab?)` · `set_editor_text(text, mode=replace|insert|new_tab)` | 쓰기(본문) | 활성/지정 탭 본문·선택·캐럿 · 본문 넣기(새 탭 기본) | `Editors` |
| `format_sql(sql?, dialect?)` | 읽기 | 포맷터(설정 프리셋) 결과 | 기존 포맷 명령 |
| `search_objects(pattern, kinds?, schema?)` · `describe_object(schema, name, kind)` · `object_source(...)` | 읽기 | 객체 찾기(findobj 엔진) · 열·키·인덱스 · 소스(보수적 생성 원칙 그대로) | `findobj_engine` · 카탈로그 `source_with` |
| `list_profiles()` · `connect(profile)` · `session_info()` | 쓰기(접속) | 프로필 **이름만** · 접속은 호스트가 볼트로(비밀번호 전달 없음) · 현재 세션·스키마·방언 | DR-22 · `Sess` |
| `run_query(sql, max_rows=200, timeout_ms)` | 읽기/쓰기 판정 | **읽기 문장만 자동**(SELECT/WITH/EXPLAIN/SHOW/DESCRIBE · 방언별 판정 · 문장 1개) · 그 밖(UPDATE/DELETE/DDL/익명 블록)은 **편집기 탭에 넣고 "실행은 사용자가 F5"** 안내만 · 실행은 `gate_pass()` → `Sess::submit`(세션 바쁘면 거부) · 결과 = 열·행 사본(행 상한 · 셀 길이 상한 · 비밀 이름 열 가림) | DR-34 · 61 §1-7 |
| `get_result(tab?, rows?)` · `explain(sql)` | 읽기 | 현재 결과 그리드 사본 · 실행 계획 | `grid` · explain 경로 |
| `attach_context(kind)` | 읽기 | 스키마 요약(DDL 목록) · 현재 탭 · 선택 영역을 프롬프트 문맥으로 | — |
| `approve`(Claude 권한 프롬프트 도구) | — | 권한 요청 → 확인 창 → `{behavior: allow|deny}` | — |

- 모든 도구 호출은 **AI 패널에 카드로 표시**(이름 · 인자 요약 · 결과 요약 · 소요) · 쓰기 등급은 Auto 모드와 무관하게 확인 창(설정 `ai.confirm_writes`는 **끌 수 없음** · 61 §1-7 사상).
- 외부 전송 확인: 행 데이터가 AI(원격)에 가는 도구(`run_query` 결과 · `get_result`)는 첫 호출 때 "이 세션 동안 행 데이터 전송 허용" 확인(스키마·열 이름은 허용 · 설정 `ai.send_rows` = ask|never|session).

## 4. 인증 · 비밀

| | Claude | Codex |
|---|---|---|
| 1차(공식) | **API 키**(`sk-ant-api…` · 콘솔 발급 · 만료 설정 권장) → 볼트 봉투 → 자식 프로세스 환경 `ANTHROPIC_API_KEY` · `--bare`(OAuth·키체인 안 읽음) · 워크스페이스 미지정 키면 `ANTHROPIC_WORKSPACE_ID`(앱 설정) | **API 키**(`OPENAI_API_KEY`) → 볼트 → app-server 환경 |
| 사용자 자체 로그인 재사용 | 비-bare 모드면 CLI가 사용자의 Claude Code 로그인을 읽을 수 있으나 **제품 기능으로 제공 금지**(Agent SDK 문서: 제3자는 claude.ai 로그인/한도 제공 불가 · 사전 승인 제외) → 설정 `ai.claude.use_cli_login`은 **두지 않는다**. 브랜딩도 "Claude Code" 표기 금지 → 패널 이름 "Claude" | app-server는 `codex login` 상태를 스스로 씀(앱은 관여 0) · "Sign in with ChatGPT"를 **앱이 구현하지 않음**(파트너 시범 · 미등록) · 엔터프라이즈 = `clientInfo.name` 기록 |
| 저장 | 새 API `Vault::put_secret(name, bytes)` / `get_secret` / `delete_secret`(`sealed::seal/open` + `devkey` · `secrets/<해시>.conf`) · 설정·프로젝트 파일 평문 금지(D-407) · 로그는 길이만 | 같음 |
| 화면 | "API 키: 설정됨(…끝 4자)" · 지우기 버튼 · 키 입력 상자 `masked` | 같음 |

## 5. 세션 · 프로토콜(요지)

- **Claude**: `claude -p --bare --output-format stream-json --input-format stream-json --include-partial-messages --verbose --model <m> --permission-mode <mode> --mcp-config <json> --permission-prompt-tool mcp__nexa__approve [--resume <id>] [--append-system-prompt <앱 역할>]`. 입력 = stdin JSONL 사용자 메시지(텍스트 + `image` 블록) · 출력 = `system/init`(session_id · tools · mcp_servers) → `stream_event`(`text_delta` · `thinking`) → `assistant`(content 블록 · `tool_use`) → `user`(`tool_result`) → `result`(텍스트 · usage · `total_cost_usd` · `permission_denials`) · `system/api_retry`(재시도 표시) · `system/permission_denied`. 세션 이어 하기 = `--resume <session_id>` 또는 `.jsonl` 경로. 중단 = SIGINT. 비용은 **추정치**로 표기.
- **Codex**: `codex app-server`(stdio JSONL) · `initialize`(clientInfo = "Nexa SQL") → `initialized` → `thread/start`(model · approvalPolicy · sandbox · dynamicTools?) → `turn/start`(input[text|image|localImage] · effort) → 알림 `item/started` · `item/agentMessage/delta` · `item/completed` · `turn/completed` · 서버 요청 `item/permissions/requestApproval` · `tool/requestUserInput` → 응답 · `thread/resume`/`fork` · `turn/interrupt`/`steer` · `model/list`. 토큰 사용량 알림 이름 = 문서 뒷부분 확인 필요.
- **공통 세션 모델**: `AiSession { provider, id(세션 ID), title, messages[], usage(누계), model, mode, attachments[] }` · 이력 = `NSQL_HOME/ai/<provider>/<id>.jsonl`(**가림 필터 뒤** · 설정 `ai.history_keep_days` · "기록 안 함" 스위치) · 프로젝트 파일에는 세션 ID 목록만(D-407).
- **가림 필터**(보내기 전·표시 전·저장 전 모두): 접속 문자열 `user/pw@host` → `ConnectSpec::redacted` · D-140 이름 규칙 변수 · API 키 접두(`sk-ant-` `sk-`) → `******` · 비밀번호가 든 프롬프트는 **전송 차단 + 안내**.

## 6. UI 구성(VS Code 확장 그림 대응)

- 활동 막대: 확장 아이콘 아래 `Claude` · `Codex` 아이콘(`act_material!` SVG · 2개 · 설정 `ai.enabled`가 꺼지면 숨김).
- 패널(좌측 · 폭 기억): 상단 = 공급자 이름 + 상태 점(실행 파일 발견/버전/키 설정 여부) · 세션 목록(TreeView · 새 세션 · 이름 바꾸기 · 삭제) · 하단 = 모델 콤보 · effort 콤보 · 모드(수동/acceptEdits/Auto) · 토큰 누계(입력/출력/캐시 · 비용 추정).
- 뷰 탭(편집기 자리): 메시지 목록(사용자/어시스턴트 블록 · 스트리밍 커서 · 도구 호출 카드(접기) · 승인 카드(허용/거부 버튼) · 오류 띠) · 하단 입력(multiline · Ctrl+Enter 보내기 · `/` 팝업 · 첨부 칩 · 붙여넣기 = 텍스트 삽입/이미지 칩) · 보내기/중단 버튼 · BusyRing.
- 마크다운: 1차 = 코드 블록(```sql …)만 고정폭+복사/편집기로 보내기 버튼 · 나머지 평문(렌더러 없음 · 2차 후보).
- 접근: 팔레트 `view.ai_claude`·`view.ai_codex`(토글) · `ai.new_session` · `ai.send` · 키맵 자리.

## 7. 안전 경계(사용자 질문 "프로필 접속 · 질의 · 암호 문자열")

| 동작 | 규칙 |
|---|---|
| 프로필 접속 | 도구에는 프로필 **이름**만 · 접속은 호스트가 볼트 봉투를 풀어 세션 생성 · AI/CLI/MCP 브리지에는 비밀번호 바이트가 가지 않는다 |
| 질의 실행 | 전부 `gate_pass()` → `Sess::submit` · 읽기 문장만 자동 · 쓰기는 편집기로 넘기고 사용자가 F5(백업·히스토리·트랜잭션 통제 그대로) · 세션 바쁘면 거부 안내 |
| 행 데이터 외부 전송 | 명시 확인(세션 단위) · 기본은 스키마만 |
| 암호가 든 문자열 | 가림 필터 3곳(전송·표시·저장) · 비밀번호 포함 프롬프트는 전송 차단 |
| 파일 시스템 | Claude `--bare` + `--allowedTools`에 파일 도구를 **주지 않음**(기본) · 작업 폴더 = 격리된 `NSQL_HOME/ai/work` · Codex `sandbox=readOnly` 기본 · "파일 편집 허용"은 설정으로 명시 |
| 콘솔 창 | 모든 spawn은 `quiet_command` · `// console-ok` 규칙 시험 통과 |

## 8. 없는 부품(새로 만드는 것) · 크기

| 부품 | 크기 | 비고 |
|---|---|---|
| `Vault::put_secret/get_secret/delete_secret` | 소 | sealed + devkey 재사용 · 시험 |
| `ai::host::CliHost`(spawn · 파이프 · 수거 · Wake · 종료) | 중 | quiet_command 확장(stdin/stdout 파이프 유지) |
| Claude stream-json 어댑터 · Codex app-server 어댑터 | 중·중 | JSON 파서 재사용 · 순수 함수 + 시험(녹화 이벤트 재생) |
| MCP 서버(GUI) + `nsql mcp-bridge` | 중 | JSON-RPC 최소 3메서드 · 도구 표 §3-2 · 승인 도구 |
| 활동 막대 항목 · 패널 · 뷰 탭 · 승인 카드 · `/` 팝업 | 대 | 기존 부품 조립 · claude-ui-rules |
| 가림 필터 · 읽기/쓰기 문장 판정 | 소 | 순수 함수 + MC/DC |
| 클립보드 이미지(CF_DIB/PNG) · 마크다운 렌더 | 중 · 중 | 2차 |
| 설정 키(§9) · i18n · 문서(94 · 39 · 26) | 소 | — |

## 9. 설정 키(docs/94 규약 · 범주 `ai`)
`ai.enabled`(off) · `ai.default_provider`(claude) · `ai.claude.exe` · `ai.codex.exe` · `ai.claude.model`(claude-opus-5-5) · `ai.claude.effort`(medium) · `ai.codex.model` · `ai.codex.effort` · `ai.mode`(manual|accept_edits|auto · 기본 manual) · `ai.send_rows`(ask) · `ai.max_rows`(200) · `ai.history_keep_days`(30) · `ai.history_enabled`(on) · `ai.max_sessions`(2) · `ai.confirm_writes`(on · 끌 수 없음 = 표시만).

## 10. 결정 후보(사용자 확인 · 추천 표시)
1. **통합 방식 = CLI 호스팅(추천)** vs 직접 API. 추천 이유 = VS Code 확장과 같은 길 · 에이전트·`/`명령·권한·세션이 CLI에 있음 · 외부 crate 0 유지. 직접 API는 2차 경량 모드.
2. **인증 = API 키만(추천)** · 사용자 CLI 로그인 재사용 기능은 두지 않음(약관) · Codex 로그인은 CLI 몫.
3. **앱 기능 노출 = MCP 서버 + 브리지(추천)** · Codex는 `dynamicTools` 우선 검토.
4. 쓰기 질의 = 항상 사용자 F5(추천) vs Auto 모드에서 승인 카드로 실행.
5. 기본 모델 = `claude-opus-5-5`(추천 · 스킬 기본) · 행 데이터 전송 기본 = 묻기(추천).
6. 1차 범위 = Claude 먼저 끝까지(§8 부품 전부) → Codex 어댑터(2주차) · 클립보드 이미지·마크다운은 3차.

## 11. 시험 계획
- 순수 함수: 가림 필터 · 읽기/쓰기 판정(MC/DC) · stream-json/JSON-RPC 파서(녹화 파일 재생) · `/` 명령 파서.
- 모의 CLI: 시험용 실행 파일(스크립트)이 녹화 이벤트를 stdout으로 흘려 호스트 수거·UI 반영을 헤드리스 캡처(`ai.dump:<파일>`)로 판정.
- MCP: 브리지 ↔ GUI 리스너 왕복(localhost · 토큰 불일치 거부) · `run_query` 읽기/쓰기 분기 · 승인 거부 경로.
- 실기: 실제 `claude`/`codex` 설치 PC에서 V1(사용자) · 콘솔 창 0 · 종료 때 자식 정리.

## 12. 참조
- Anthropic 인증: <https://platform.claude.com/docs/en/manage-claude/authentication> · Agent SDK 개요(제3자 claude.ai 로그인 불가 · 브랜딩): <https://code.claude.com/docs/en/agent-sdk/overview> · headless 프로토콜: <https://code.claude.com/docs/en/headless>
- Codex app-server: <https://learn.chatgpt.com/docs/app-server> · MCP 설정: <https://learn.chatgpt.com/docs/extend/mcp?surface=cli> · Codex 인증: <https://developers.openai.com/codex/auth> · Sign in with ChatGPT(파트너): <https://developers.openai.com/siwc/website>
- 저장소: 확장 ABI docs/75 · 활동 막대 `activity.rs` · 네트워크 `nsql-net` · 볼트 `nsql-vault` · 세션 통제 docs/52 · 데이터 보호 61 §1-7 · 변수 가림 D-140/T-132.
