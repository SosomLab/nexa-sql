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
| 사용자 자체 로그인 재사용(별도 API 키 없이) | **기술적으로 가능**: 비-bare 모드면 CLI가 사용자 PC의 Claude Code 로그인(claude.ai 구독)을 스스로 읽는다 · 앱은 토큰을 만지지 않는다. **약관**: Agent SDK 문서 = 사전 승인 없는 제3자 제품은 claude.ai 로그인/한도를 제공할 수 없다 → 설정 `ai.claude.use_cli_login`(**기본 off** · 켜면 안내문 "개인 사용 · 배포판에서는 Anthropic 승인 필요" · 사용자 11-11 질문 "별도 API 키 없이 연계 가능한가" = 가능하되 이 조건) · 브랜딩 "Claude Code" 표기 금지 → 패널 이름 "Claude" | app-server는 `codex login`(ChatGPT 계정) 상태를 스스로 씀(앱 관여 0) → 별도 키 없이 동작 · "Sign in with ChatGPT"를 **앱이 구현하지 않음**(파트너 시범) · **확인됨(app-server 문서)**: "app-server 인증은 상업·호스팅 서비스에는 허용된 적이 없다 · 기존 로컬/오픈소스 앱은 계속 쓸 수 있으나 Sign in with ChatGPT로 이전 권장" → Claude와 같은 꼴 = 설정 `ai.codex.use_cli_login` 기본 off + 안내 · 로그인 상태는 `account/read`(`requiresOpenaiAuth`) · `account/updated`(authMode · planType) · `account/login/start`(type = apiKey · chatgpt · chatgptDeviceCode) · 토큰 = `thread/tokenUsage/updated` · 한도 = `account/rateLimits/updated` · 승인 = `item/commandExecution/requestApproval` · `item/fileChange/requestApproval`(accept · acceptForSession · decline · cancel) · 동적 도구 호출 = `item/tool/call`(실험) |
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
2. **인증 = API 키 기본 + `ai.claude.use_cli_login` 기본 off(추천 · 1분 규칙으로 채택)** — 개인 사용은 설치된 CLI 로그인으로 키 없이 동작 · 배포판은 약관 안내 · Codex도 같은 꼴(`ai.codex.use_cli_login` 기본 off · app-server 문서 "상업·호스팅 서비스에는 불허 · 로컬 앱은 가능").
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

## 13. 약관 · 비용 · 한도 검토(2026-11-11 · 사용자 "CLI를 쓴 개발·배포가 문제 없는가 · API 비용·한도" · Claude Ultra 구독 · Codex Free)

### 13-1 약관(공식 원문 기준 · 법률 자문 아님)
| | Anthropic(Claude) | OpenAI(Codex) |
|---|---|---|
| 적용 약관 | 구독(Free/Pro/Max/Ultra) = 소비자 약관 · API·Agent SDK·제품 내 Claude Code = **상업 약관**(code.claude.com/docs/legal-and-compliance) | 구독(Free/Go/Plus/Pro) = Terms of Use · API·개발자 = **Services Agreement(Business Terms)** |
| 제품이 CLI를 띄워도 되는가 | **된다(조건부)**: "제품 안에서 Claude Code를 미리 설치하거나 실행하려면 상업 약관 동의 + ① **바이너리 무수정**(인증 방식 제거·제한 금지) ② **사용량을 대신 결제·재판매·중개하지 않는다** — 최종 사용자가 **자기** API 키·구독 자격·3P 자격으로 인증" · "최종 사용자가 **무수정 Claude Code에 자기 구독으로 로그인하는 것은 막지 않는다**" | app-server 문서: "**app-server 인증은 상업·호스팅 서비스에는 허용된 적이 없다** · 기존 로컬/오픈소스 앱은 계속 쓸 수 있으나 Sign in with ChatGPT로 이전 권장" · API 키 경로는 Services Agreement(키 매매·양도 금지 3.3(g) · 사용 한도 우회 금지 3.3(h)(i)) |
| 금지되는 것 | "제3자 개발자가 **자기 앱 안에 Claude.ai 로그인을 제공**하거나 Free/Pro/Max/Ultra **자격으로 사용자 대신 요청을 라우팅**하는 것 · Claude.ai 자격·세션 토큰을 **수집·저장·중개**하는 것 — 로그인은 Anthropic 자체 흐름으로만" · "OAuth는 구독 구매자의 **일상적 사용** 전용 · 광고된 한도는 Claude Code·Agent SDK의 **통상적 개인 사용**을 전제" · 브랜딩: "Claude Code"·Anthropic 이름/로고를 제품·기능명·로고에 쓰거나 제휴로 보이게 금지(평문으로 "Claude Code를 실행한다"는 표기는 허용) | Terms of Use "What you cannot do": 서비스 수정·복제·재판매 · 리버스 엔지니어링 · **"Output을 자동·프로그램으로 추출"** · 산출물을 사람이 만든 것으로 표시 · 요율 제한·보호 장치 우회 · Output으로 경쟁 모델 개발 · 계정 자격 공유(Business 3.1) · 사용 정책: "민감 영역(금융·법률·의료·고용…)의 **고위험 행위를 사람의 검토 없이 자동화** 금지" |
| 우리 설계와의 대조 | ✅ 앱은 설치된 `claude`를 **무수정**으로 띄우고 토큰을 **만지지 않으며**(저장·중개 0) 로그인은 CLI 자체 흐름 · 사용량은 사용자 본인 계정에 과금(앱이 대신 결제·재판매 0) · 패널 이름 "Claude"(브랜딩 규칙) → **허용 범위 안**. ⚠ 단 "자기 앱 안에 로그인 제공 금지"와의 경계: 앱이 로그인 버튼을 두고 CLI의 `/login`을 **대신 호출·안내만** 하는 것까지는 Anthropic 흐름이지만, 토큰을 받아 보관하는 순간 위반 → 설계 §4 유지(앱은 토큰 0). ⚠ Ultra 한도는 "통상적 개인 사용" 전제 → 자동화 대량 호출(Auto 모드 루프)은 한도 축소·조치 대상이 될 수 있음 → 기본은 대화형 · Auto는 사용자 세션 안에서만 | ⚠ app-server를 제품(배포판)이 ChatGPT 로그인으로 쓰는 것 = "상업·호스팅 서비스 불허" → **배포판 기본 = API 키** · 개인 사용(로컬 앱)은 명시적으로 계속 허용 · Terms of Use의 "Output 자동 추출 금지"는 ChatGPT 웹/앱 스크래핑을 겨냥한 것으로, app-server는 OpenAI가 **제공하는 프로토콜**이라 "허용된 수단"(Business 3.3(f) "서비스가 허용한 방법 외 추출 금지"의 반대 해석) · 사용 정책 "고위험 자동화 금지"는 **쓰기 질의 = 사용자 확인** 설계(§7)로 충족 |
| 결론 | **개발·배포 가능**(상업 약관 동의 · 무수정 · 자기 자격 · 토큰 비보관 · 브랜딩 준수). 구독 재사용은 "기능으로 제공"이 아니라 "사용자가 자기 CLI에 로그인해 두면 동작"으로 두고 배포판 안내문에 위 조건을 적는다 | **개발·배포 가능(API 키 경로)** · ChatGPT 로그인 재사용은 개인 사용 한정(설정 기본 off + 안내) · 파트너 등록(Sign in with ChatGPT)을 받으면 배포판에서도 허용 |

### 13-2 비용 · 한도
**Claude**
- 구독(claude.com/pricing): Ultra **$200/월** = Pro의 **10×** 주간 한도 · **5시간 한도 없음(검증 계정)** · Fable은 주간 한도의 50%까지 · **월 $200 API 크레딧 포함**(Claude Platform용 · 별도 API 키로 쓰는 크레딧). 메시지·토큰 수치는 비공개("모델·대화 길이·파일 크기에 따라 다름"). Max $100 = 5× · $100 크레딧 · Pro $20.
- API(platform.claude.com · USD/1M 토큰 입력/출력): **Opus 5.5 $4/$20**(캐시 읽기 $0.20) · Sonnet 5.5 $2/$10 · Haiku 5.5 $0.10/$0.50 · Fable 5.1 $10/$50 · 1M 컨텍스트 · 프롬프트 캐시 읽기는 ITPM에 **안 셈**.
- API 등급(월 지출 상한 · 자동 승급): Start $500 · Build $1,000 · Scale $200,000 · Custom 무제한 · 신규 조직은 Evaluation 등급(더 낮음)에서 시작. 분당 한도 Start: Opus 5.5/Sonnet 5.5/Haiku 5.5 = 4,000 RPM · 2,000,000 ITPM · 400,000 OTPM · Fable 5.1 = 4,000 · 500,000 · 100,000. 429 = `retry-after`(지출 상한 429는 헤더 없음 · `enforced_spend_limit_reached`).
- 환산(예): 스키마 20K 토큰 + 질문 1K → 응답 2K를 Opus 5.5로 1회 ≈ $0.08(캐시 적중 시 입력 ≈ $0.004) · 하루 100회 ≈ $8(캐시 없이) — Ultra 포함 크레딧 $200/월로 상당 부분 충당 가능.

**OpenAI / Codex**
- 구독(learn.chatgpt.com/docs/pricing): Codex는 Free·Go·Plus·Pro·Business·Edu·Enterprise에 포함. **Free($0) = "빠른 코딩 작업 탐색" · 데스크톱 앱에서 GPT-6 Luna(표준 속도) · 기능표에 Free/Go의 CLI·IDE 확장·app-server 지원은 없음**(제3자 정리: Free는 CLI 미포함 · Plus부터) · Plus $20 = 웹·CLI·IDE·iOS + GPT-6.1 Sol/Luna · 5시간 창당 추정 메시지(Plus): Astra 5~45 · 6.1 Sol 15~160 · Luna 350~3,000 · 주간 한도 별도 · Pro $100/200/500(5시간 한도 없음). 초과분 = 크레딧(예: 6.1 Sol 입력 50 / 출력 250 크레딧 per 1M).
- API(developers.openai.com/api/docs/pricing · USD/1M 입력/캐시/출력): **gpt-6.1-sol $2/$0.10/$10** · gpt-6-astra $10/$1/$50 · gpt-6.1-terra $1/$0.05/$5 · gpt-6-luna $0.10/$0.01/$0.50 · gpt-5.5 $5/$0.50/$30 · gpt-5.4 $2.50/$0.25/$15 · gpt-5.3-codex $1.75/$0.175/$14 · 272K 초과 장문은 할증 · Fast 모드 2×. "API 키로 로그인하면 표준 API 요율"(learn.chatgpt.com/docs/auth).
- API 등급(크레딧 누적 구매로 자동 승급 · 월 사용 상한): **Free $100/월(허용 지역)** · Build($5 구매) $500 · Launch($100) $5,000 · Grow($500) $200,000. 분당 한도 Build: Astra/Sol/Terra 5,000 RPM · 1,000,000 TPM · Luna 2,000,000 TPM(Free 등급 수치는 비공개). 429 = `Retry-After` + 지수 백오프 · `slow_down`(급증) · 실패 요청도 분당 한도에 산입.
- **사용자 상황(Codex Free)**: Free 플랜으로는 CLI·app-server 경로가 기능표에 없어 **이 플러그인(app-server) 사용이 사실상 불가** → API 키(종량) 또는 Plus 이상 필요. API 키면 gpt-6.1-sol 기준 위 Claude 예와 같은 1회 ≈ $0.06.

### 13-3 설계 반영
- 배포판 안내문(설정 ▸ AI): "이 기능은 사용자 PC에 설치된 무수정 `claude`/`codex`를 실행합니다 · 인증·과금은 사용자 본인 계정 · 앱은 토큰을 보관하지 않습니다 · Anthropic/OpenAI 약관 적용" + 각 링크.
- `ai.claude.use_cli_login`·`ai.codex.use_cli_login` 기본 off · 켤 때 위 조건 안내(Codex는 "개인 사용 한정 · 배포판 불허").
- Auto 모드 호출 상한 `ai.auto_max_turns`(기본 20) · 한도 429 수신 시 자동 재시도 금지(26 §8) · 비용 추정 표시(Claude `total_cost_usd` · Codex 토큰 × 요율표).
- 참고 링크: Claude Code 법적 안내 <https://code.claude.com/docs/en/legal-and-compliance> · 상업 약관 <https://www.anthropic.com/legal/commercial-terms> · 소비자 약관 <https://www.anthropic.com/legal/consumer-terms> · 사용 정책 <https://www.anthropic.com/legal/aup> · 요금 <https://claude.com/pricing> · API 요율 제한 <https://platform.claude.com/docs/en/api/rate-limits> · OpenAI 이용 약관 <https://openai.com/policies/terms-of-use/> · Services Agreement <https://openai.com/policies/services-agreement/> · 사용 정책 <https://openai.com/policies/usage-policies/> · Codex 요금 <https://learn.chatgpt.com/docs/pricing> · Codex 인증 <https://learn.chatgpt.com/docs/auth> · app-server <https://learn.chatgpt.com/docs/app-server> · API 가격 <https://developers.openai.com/api/docs/pricing> · API 요율 제한 <https://developers.openai.com/api/docs/guides/rate-limits>.

## 14. 기술 검토 추가(2026-11-11 · 사용자 질문 4건 · 구현 전)

### 14-1 API 모드와 CLI 모드를 같게 만들 수 있나
- **화면·조작은 같게, 능력 범위만 다르게.** 패널·뷰는 `AiEvent`(글자 조각 · 도구 호출 카드 · 승인 요청 · 토큰/비용 · 끝)만 보고 그린다 · `AiHost` 포트 뒤에 `CliHost`(claude -p stream-json)와 `ApiHost`(Messages API SSE + 자체 도구 루프)를 꽂는다 → 대화·스트리밍·모델/effort·`/` 명령·첨부·토큰 표시·승인 카드·DB 도구·Auto 모드는 **같은 코드**.
- 같은 것: 채팅·스트리밍 · 모델/effort · 토큰·비용 · 첨부(텍스트·이미지·PDF) · `/` 명령 · 앱 기능 도구(테이블 검색·조회·포맷·설정 변경) · Auto(읽기 자동·쓰기 확인).
- 다른 것(모드 배지 · 비활성 표시): CLI만 = 파일 시스템·셸 도구 · 서브에이전트 · Claude Code 스킬 · 구독 로그인(개인 사용) · `--resume` 세션 재개. API만 = 서버 측 압축·캐시 제어 · 키별 과금·워크스페이스.
- **앱 기능 도구 표는 두 모드가 공유**: CLI = MCP로 노출(§3) · API = 같은 표를 `tools`(strict 스키마)로 전달하고 호출은 in-process(브리지 불필요). 도구 루프 = 수동 루프(`stop_reason == tool_use` · 병렬 도구 결과는 한 메시지 · `is_error` 보고 · 강제 tool_choice 금지 · Opus 5.5 effort 기본 medium 명시).

### 14-2 좌측 Claude 탭 + 상단 모드 선택(API | CLI) + 그 아래 VS Code식 구성(사용자 그림 1)
- 상단: 모드 세그먼트(API | CLI) · 상태 점(실행 파일/키/프로필) · 세션 목록 접기.
- 본문: 대화 기록(사용자 말풍선 · 어시스턴트 블록 · 도구 호출 접기 카드 · 승인 카드 · 오류 띠) — `ext_view`식 읽기 전용 상자 + 카드.
- 입력부(그림 하단 바): `+`(첨부 메뉴 = 클립보드·파일·**테이블 조회**) · `/`(명령 팝업) · 경과 시간 · 에이전트 수(CLI 서브에이전트 · API = 0) · 모델/effort 칩(콤보) · Auto 토글 · 보내기/중단 · BusyRing.
- 제약: 마크다운 렌더(1차 = 코드 블록만 고정폭+복사) · 클립보드 이미지 읽기(2차) · 모두 기존 Button·Combo·Switch·TextBox·팝업 부품.

### 14-3 API 여러 개 등록·전환
- `ai.profiles`(설정 레지스트리 밖 · `NSQL_HOME/ai/profiles.conf` · D-407 예외 아님 = 프로젝트 파일이 아니라 앱 데이터) = [{이름 · 공급자(claude · openai · anthropic 호환 게이트웨이 = Bedrock/Vertex/Foundry/사내 프록시) · base_url · 키 참조(볼트 봉투 이름) · 워크스페이스 ID · 기본 모델 · 기본 effort}] · 패널 상단 콤보로 전환 · **세션 = 프로필 고정**(대화 이력은 공급자 간 호환 안 됨 · 전환은 새 세션부터) · 키는 볼트에만 · 설정 파일엔 이름·엔드포인트만 · 요율 제한·지출 상한 429는 프로필 단위로 표시(재시도 금지 · 26 §8).

- ★ **HTTP 오류 표시(11-13 · bf5633e8)** = 대본 Error = `HTTP <상태>: <error.message | 본문 앞 400자 | "(서버가 오류 내용을 보내지 않았습니다)">` + ` (request-id …)` + 상태별 안내 한 줄(5xx/529 = 일시 장애·DB엔 아무것도 실행 안 됨 · 429 = 한도 · retry-after 초 · 401/403 = 키·워크스페이스·모델 권한) · 종전 = SSE 싱크가 오류 본문(JSON · SSE 아님)을 버려 "HTTP 503: " 뒤가 비었음 · 자동 재시도 = 없음(사용자 답 대기 · 넣으면 [26 §8](26-performance-architecture.md) 등재) · 협업 판정 = 로컬 모의 서버 ✓([journal 11-13 §17](journal/2026-11-13.md)).

### 14-4 API 모드의 입력 데이터
| 입력 | 방법 | 비고 |
|---|---|---|
| 클립보드 텍스트 | 입력 상자 붙여넣기(기존 `read_text`) | 긴 글은 "첨부 칩"으로 접음(≥ 2,000자) |
| 클립보드 이미지 | nexa-ctl에 CF_DIB/PNG 읽기 추가 → `image` 블록(base64 · PNG/JPEG) | 2차 |
| 파일 | 파일 대화상자·드롭 → 텍스트/SQL/CSV = 텍스트 블록(크기·행 상한) · PDF = `document` 블록(≤32 MB · 600쪽) · 이미지 = `image` · 큰 파일 = Files API 업로드 후 `file_id` 재사용 | 미리보기 칩 · 제거 가능 |
| **테이블 조건 조회** | `+` ▸ "테이블 조회 첨부": 스키마.테이블 · WHERE · 행 상한(기본 200) 입력 → 호스트가 `gate_pass()`→`Sess::submit`으로 **읽기 질의만** 실행 → 결과를 열 이름 + 행의 CSV/표 텍스트 블록으로 첨부(셀 길이 상한 · 비밀 이름 열 가림 · 행 수 표시) · 또는 Auto 모드에서 AI가 `run_query` 도구로 직접 | 외부 전송이라 첫 사용 때 "행 데이터 전송 허용"(세션 단위) 확인 · 프롬프트 캐시 경계 뒤에 두어 반복 질문 비용 절감 |

### 14-5 Workload Identity Federation(ID 페더레이션)을 우리도 쓸 수 있나(사용자 11-11)
- **쓸 수 있다 — 단 "조직 환경"의 이야기.** WIF = 조직 관리자가 Claude Console에 ① 서비스 계정(`svac_`) ② 페더레이션 발급자(`fdis_` · 조직 IdP의 `iss` URL + JWKS) ③ 규칙(`fdrl_` · subject/audience/claims/CEL 조건 → 어느 서비스 계정 · 범위 `workspace:developer` · 토큰 수명 60~86,400초)을 만들어 두면, 클라이언트가 IdP의 **JWT**를 `POST /v1/oauth/token`(`grant_type = urn:ietf:params:oauth:grant-type:jwt-bearer` · `assertion` · `federation_rule_id` · `organization_id` · `service_account_id` · `workspace_id`)로 바꿔 단명 토큰(`sk-ant-oat01-…` · `Authorization: Bearer`)을 받는다. 정적 API 키 배포 없이 사용 · 과금은 조직 워크스페이스(서비스 계정)로.
- 지원 IdP: AWS IAM · Google Cloud · Microsoft Entra ID · GitHub Actions · Kubernetes · SPIFFE · Okta · **임의 표준 OIDC 발급자**(Custom OIDC).
- 데스크톱 앱(이 플러그인)에서의 형태: 사용자가 회사 IdP(Entra ID·Okta·사내 OIDC)에 **브라우저 로그인(PKCE · localhost 콜백 — 부품 있음)** → 받은 ID 토큰(JWT)을 위 교환 요청으로 Anthropic 토큰으로 → 1시간 안팎마다 재교환. 규칙이 "이 발급자의 이 클레임이면 서비스 계정 Z"라 **사용자 신원은 IdP가, 과금·권한은 서비스 계정**이 맡는다(감사는 IdP 로그 + Console 인증 이력).
- 주의: ① `jti`가 있는 JWT는 **1회용** — 재교환마다 IdP에서 새 토큰을 받아야 한다(refresh_token/silent 재발급 필요 · 캐시 재사용 = `jti_reused` 실패) ② 발급자 JWKS는 공개 https여야(사내 IdP가 비공개면 `inline` 키 업로드) ③ 관리자 설정 없이는 불가 · **개인 Ultra 구독과는 무관**(Console 조직 전용) ④ OpenAI API의 동등 기능은 문서에서 확인하지 못함(확인 필요).
- 설계 반영: `ai.profiles`의 인증 종류에 `api_key | wif`를 둔다 · `wif` = {발급자 로그인 URL(OIDC 디스커버리) · client_id · 규칙/조직/서비스 계정/워크스페이스 ID} · 토큰·refresh_token은 볼트 봉투 · 교환·갱신은 `nsql-net` POST(SSE 아님) · 2차 범위(1차 = API 키).

## 15. 구현 1차 기록(2026-11-11 · 사용자 "전체 기능을 이번 사이클에" · 커밋 be20dc72 · 2d8adfaa)

### 15-1 무엇이 들어갔나(코드 = `crates/nexa-sql/src/ai/` + `app/ai.rs` · 배선 34파일 · +8,002줄)
| 조각 | 파일 | 요지 |
|---|---|---|
| 핵심 모델 | `ai/mod.rs` | `Mode{Api,Cli}` · `Profile`/`ProfileStore`(`NSQL_HOME/ai/profiles.conf` · 키 없음) · `Session`(메시지 · 사용량 · CLI 세션 id · 첨부 · 행 동의) · 이력 `NSQL_HOME/ai/history/<id>.jsonl`(**가림 뒤 저장** · 보존 `ai.history_keep_days`) · `redact`/`contains_secret`(sk- 키 · `password=` · `user/pw@`) · `AiEvent`(호스트 → UI) |
| CLI 프로토콜 | `ai/claude_cli.rs` | `claude -p --output-format stream-json --input-format stream-json --include-partial-messages --verbose --model --permission-mode [--bare] [--effort] [--mcp-config <인라인 JSON> --permission-prompt-tool mcp__nexa__approve] [--resume] --append-system-prompt` · stdout 줄 → 이벤트(init/텍스트 델타/tool_use/tool_result/result 사용량·비용) · stdin 턴 = 텍스트 + 이미지 블록(base64 자체 구현) |
| API 직접 | `ai/claude_api.rs` | Messages API(`nsql-net` POST · SSE 싱크로 블록 조립 — 사고·서명·tool_use 입력 델타 포함) · `thinking: adaptive` · `output_config.effort` · 프롬프트 캐시(system + 마지막 메시지 `ephemeral`) · 도구 루프(≤64회 · 도구 실행은 UI 스레드에 `ToolRequest`로 부탁) · 비용 추정(요율 표) · `/v1/models`로 키 검증 |
| 도구 표 | `ai/tools.rs` | 16종(§3-2의 축약판 · 이름은 접두 없이 · MCP는 `mcp__nexa__*`): list_commands · run_command · get_setting · set_setting · get_editor_text · set_editor_text · format_sql · search_objects · describe_object · object_source · list_profiles · session_info · connect · run_query · get_result · explain · (+MCP `approve`) · `write` 표식 = 승인 카드 대상 · 시스템 프롬프트 공통 |
| 호스트 | `ai/host.rs` | `Host{tx,rx,running}` — CLI = `quiet_command`(콘솔 창 0 · Windows npm `claude.cmd` → `node cli.js`로 풀어 셸 없이) 자식 + stdout 읽기 스레드 + Wake · API = 턴 스레드(대화 `Arc<Mutex<Vec<Json>>>` 공유) · Mock(`NSQL_AI_MOCK=1` · 헤드리스 V1 각본) · ■ = kill/깃발 |
| MCP 서버 | `ai/mcp.rs` + `nsql-cli/src/mcp_bridge.rs` | GUI가 `127.0.0.1:<ai.mcp_port>`(0 = 임의)에 줄 JSON-RPC(initialize · tools/list · tools/call · ping) · 첫 줄 토큰 · `nsql mcp-bridge --port --token` = stdio ↔ TCP 중계(Claude Code가 띄움) · `approve` = 앱 승인 카드 → `{behavior: allow|deny}` |
| 좌측 패널 | `ai/panel.rs` | 모드 분할 버튼(API/CLI) · 프로필/모델/노력도 드롭 · Auto 스위치 · [새 대화][프로필] · 사용량 한 줄 · **세션 관리**(사용자 11-11 "이전 기록 활용"): 검색 상자 · 목록(모드 태그 · 제목 · 턴 수) · 클릭 = 열기/이어하기 · 우클릭 = 열기·이름 바꾸기·삭제 · 메뉴 비트 131072 |
| 채팅 뷰 탭 | `ai/view.rs` | 뷰 탭 키 `ai.chat`(편집기 자리 · UI 글꼴) · 머리줄(모드·모델·프로필·상태) · 대본(역할 줄 · 단어 줄 바꿈 캐시 · 코드 블록 배경 + [복사][편집기에 넣기] · 도구 호출/결과 줄 · 승인 카드 [허용][거부] · 오류 경고색) · 작성 상자(다중행 · Enter 보내기 · Shift+Enter 줄 바꿈 · Esc 비움) · `/` 팝업(11 명령 · ↑↓ · Tab/Enter 완성) · 첨부 칩(× 제거) · [파일…][클립보드][프로필] [보내기/■] · 두 번째 페이지 = 프로필 관리(목록 [사용][편집][삭제] · 폼 이름/기본 URL/워크스페이스/모델/노력도/**키 마스킹** · [저장][검증][대화로] · 키 보관 안내) |
| 앱 연결 | `app/ai.rs` | `AiState`(호스트 · 세션 목록 · 프로필 저장소 · MCP · 승인 대기 · 비동기 도구 대기 · 결과 포착) · 세션 관리(새 대화 · 열기 = CLI `--resume` / API = 글만 재구성 · 이름 · 삭제) · 보내기(비밀 포함 글 차단 · 첨부 본문화/이미지 블록) · 이벤트 수거(틱 + Wake) · 도구 실행 한 곳(읽기 = 즉시 · 쓰기 = 승인 · 행 데이터 = 세션당 1회 동의 · Auto = 읽기·안전 명령 자동 · **비조회 run_query는 Auto여도 확인**) · `run_query`/`explain`/`/query` = `run_text_whole` → `run_events`에서 결과셋 포착 → 세션 한가해지면 응답(`ai.max_rows` 상한 · 오류는 상태줄 글) · `object_source` = 탐색기 소스 탭 열림 대기 · 파일 첨부(이미지/PDF/글 ≤512 KB) · 헤드리스 `ai.open` `ai.panel` `ai.send:` `ai.approve:` `ai.dump:` |
| 배선 | activity · commands · menus · input · paint · event_loop · project · toolbar · settings · init · startup_cmd · run_events · grid(`ai_text`) · explorer(`ai_search_objects`/`ai_describe`/`ai_object_info`) · file_win/files(`FilePurpose::AiAttach`) | 활동 막대 `view.ai`(`ai.enabled` 꺼지면 숨김) · 팔레트 `tv("view.ai")` · `p("ai.chat")` · `sw("ai.enabled")`(확장 관리자처럼 켜기/끄기 · 사용자 11-11) · 포커스 `Focus::Ai` · 프로젝트 패널 복원 `"ai"` |
| 설정 | `nsql-settings` 범주 `CatAi` 13키 | `ai.enabled`(off) · `ai.mode`(api|cli) · `ai.model`(claude-opus-5-5) · `ai.effort`(medium) · `ai.auto`(off) · `ai.max_rows`(200) · `ai.timeout_secs`(300) · `ai.max_tokens`(8192) · `ai.history_keep_days`(90) · `ai.cli_path`(claude) · `ai.permission_mode`(default|acceptEdits|plan) · `ai.bare`(on) · `ai.mcp_port`(0) |

### 15-2 §10 결정 후보의 실제 선택(추천안 그대로 · 1분 규칙)
- 키 보관 = 볼트 봉투 `ai.<프로필>`(`Vault::put_secret` · 기기 키 · 설정/프로젝트 파일 0) · 입력은 마스킹 · 검증은 `/v1/models` · 자식 프로세스에는 환경 변수로만(1차 CLI 모드는 사용자 자신의 로그인이라 키를 넘기지 않는다).
- CLI 모드 기본 `--bare`(사용자 홈 훅·MCP·CLAUDE.md 미적재) · 파일 도구는 주지 않음(`--permission-prompt-tool` = 우리 `approve`).
- 행 데이터 = 세션당 1회 동의 카드(`rows_allowed`) · `/rows on|off` · `/query`는 동의로 간주.
- 이력 = 가림 뒤 저장 · 세션 제목도 가림.
- 모델 선택지 4(opus-5-5 · sonnet-5-5 · haiku-5-5 · fable-5-1) · Models API 조회는 2차.

### 15-3 구현 2차(2026-11-11 · 커밋 8c13ea52) — Codex · 제공자 선택 · 클립보드 이미지 · V1 수정
- **제공자**: `Provider{Claude,Codex}` — 세션·프로필에 들어가고(이력 jsonl `provider`) 패널 맨 위 분할 버튼 [Claude|Codex] · Claude면 그 아래 [API|CLI] · Codex면 모드 행 없이 프로필 행(API 키) · 설정 `ai.provider`.
- **Codex 어댑터**(`ai/codex.rs` + `host.rs` `Running::Codex`): `codex app-server`(stdio JSONL · `jsonrpc` 생략) · 악수 = `initialize`(clientInfo nexa-sql · `experimentalApi`) → `initialized` → `account/login/start`(apiKey = 프로필 키 · `ai.codex_cli_login`이 켜져 있고 키가 없으면 생략) → `thread/start`(model · cwd = `NSQL_HOME/ai/codex-cwd` · `approvalPolicy: untrusted` · `sandbox: read-only` · **`dynamicTools` = 우리 도구 표 16종**) 또는 `thread/resume`(세션의 스레드 id) → `turn/start`(text + `localImage` 경로 · effort · 첫 턴은 스레드 준비 뒤 읽기 스레드가 보냄). 알림 → `AiEvent`(`item/agentMessage/delta` · `item/started|completed` 도구 · `thread/tokenUsage/updated` · `turn/completed` · `error`) · 서버 요청 `item/tool/call` → UI 도구 실행 → `{contentItems:[inputText], success}` · `item/commandExecution|fileChange/requestApproval` → 승인 카드 → `decision accept|decline` · `item/permissions/requestApproval` → 빈 허용. ■ = `turn/interrupt`(프로세스 유지). 모델 선택지 `gpt-6.1-sol` · `gpt-6.1-sol-mini` · 비용 추정 요율(2/10 · 0.5/2.5).
- 약관 반영: Codex 기본은 **API 키 프로필** · CLI 자체(ChatGPT) 로그인 재사용은 설정 `ai.codex_cli_login`(기본 off · "App-server authentication has never been permitted for commercial or hosted services" · §13-1).
- **클립보드 이미지**: `clipboard::read_image_png`(Windows `CF_DIB` 24/32bpp BI_RGB/BITFIELDS → `png::encode` · `decode_dib` 순수 함수 시험) — [클립보드]/`/clip`은 글이 없으면 이미지를 첨부.
- V1 보정(협업 판정): `ai.open` = 패널 + 대화 탭 · 대화 탭 제목 `[RO]` 없음(ext 뷰와 같은 규칙) · 덤프 `tool_call` 줄에 입력 JSON.
- 설정 4키 추가(합 17): `ai.provider` · `ai.codex_path` · `ai.codex_model` · `ai.codex_cli_login`.

### 15-4 남은 것
1. 실기 확인(사용자): API 프로필 등록·[검증] · Claude CLI 모드 첫 턴(MCP 중계 `nsql mcp-bridge` 연결 `init mcp=[nexa:connected]`) · Codex 첫 턴 · 승인 카드 · `/query` · 클립보드 이미지.
2. Codex 미확인 가정(공식 문서 본문에 없어 관대하게 파싱): `item/agentMessage/delta`의 `delta` 키 · `tokenUsage.total.{inputTokens,cachedInputTokens,outputTokens}` · `item/tool/call` params(`tool`/`arguments` 또는 `name`/`input`) · dynamicTools 스키마 키 `inputSchema` — 실기에서 어긋나면 `codex.rs` 한 곳 수정(`codex app-server generate-json-schema`로 확정 가능).
3. 결과 그리드 **선택 영역** 첨부 · Models API 조회 · 비용 표 설정화 · WIF(§14-5) = 2차.

### 15-5 구현 3차(2026-11-11 · 사용자 "완료되지 않은 기능 계속 구현" · 커밋 742fb7b2 · ed6f43a2 · 40153854 · WIF 커밋)
§15-4 ②·③의 2차 항목을 전부 구현했다(남은 것 = 사용자 실기 T-475뿐).

| 항목 | 구현 | 어디 |
|---|---|---|
| **Codex 키 확정** | `codex app-server generate-json-schema`(codex-cli 0.167.0 · `target/ai-codex-schema/`)로 §15-4 ② 4가지 전부 확인 — `item/agentMessage/delta.delta` ✓ · `thread/tokenUsage/updated.tokenUsage.total.{inputTokens,cachedInputTokens,outputTokens}` ✓ · `item/tool/call` = `{tool, arguments, callId}` → 응답 `{contentItems[{type:inputText,text}], success}` ✓ · 승인 응답 `decision = accept/decline` ✓ · `error` 알림 `{error.message, willRetry}` ✓. **유일한 수정** = `DynamicToolSpec`에 `type:"function"` 필수(빠져 있었음). 주의 = 스키마에 `dynamicTools` 필드 자체는 없음(실험 API 필드는 스키마에서 빠짐 · 공식 문서대로 `thread/start`에 그대로 보냄 · 실기에서 거부되면 §15-4 ①에 기록). | `ai/codex.rs` |
| **Models API 조회** | 프로필 [검증] = 키 확인 + 모델 목록: Claude `/v1/models?limit=100` → `data[].id` · Codex = `codex app-server`를 잠깐 띄워 initialize → (apiKey 로그인) → `model/list` → `data[].{id,hidden,isDefault}`(숨김 제외 · 기본 먼저) → 종료(30초 상한). 목록은 프로필 `models=`(profiles.conf)에 저장 · 패널 모델 콤보 = 프로필 목록(없으면 내장 표 · 현재 모델 항상 포함). | `claude_api::list_models` · `host::codex_list_models` · `codex::model_list/model_ids` · `Profile::model_choices` · `PanelState.models` |
| **비용 표 설정화** | `ai.prices`(Text · 기본 `nsql_settings::AI_PRICES_DEFAULT` = `fable=10/50;mythos=10/50;gpt-6.1-sol-mini=0.5/2.5;gpt-6=2/10;opus-5-5=4/20;opus=5/25;sonnet-4=3/15;sonnet=2/10;haiku-4=1/5;haiku=0.1/0.5;*=4/20`) — `<모델 부분 문자열>=<입력>/<출력>` USD/1M · `;`/줄 바꿈 구분 · 앞에서부터 첫 일치 · `*` = 그 밖 · 오타 항목 무시. API 턴 스레드에도 표를 넘긴다(`ApiLaunch.prices`). | `mod.rs price_per_m_in/estimate_cost_in` · `nsql-settings` |
| **마크다운 렌더(2차)** | 한 줄 변환 `md_line`: `#` 제목 = 굵게(`LineKind::Head`) · `-`/`*`/`+`/`1.` 목록 = `•`/번호 + 들여쓰기(체크박스 ☐/☑) · `>` 인용 = 흐리게 `│` · `---`/`***` = 구분선 · `**굵게**` 표식 제거(짝일 때만 · SQL `a**2` 보존) · 인라인 코드 백틱·표(`|`)는 그대로 · 코드 블록 안·도구 결과에는 적용 안 함. 어시스턴트 역할 줄 = 제공자 이름(Claude/Codex). | `ai/view.rs` |
| **긴 붙여넣기 = 첨부 칩** | AI 작성 상자에 붙여넣는 글이 `ai.paste_chip_chars`(기본 2000 · 0 = 끔)자 이상이면 본문 대신 첨부 칩(`pasted (N lines)`) · 비밀 패턴이면 차단. 가로채는 곳 = 편집 메뉴/Ctrl+V 공통 경로(`EditCtxAction::Paste` → `App::ai_paste_as_chip`). | `app/menus.rs` · `app/ai.rs` |
| **WIF(§14-5)** | 프로필 인증 = `API 키 ↔ ID 페더레이션(WIF)` 토글(Claude 전용). WIF 프로필 = {OIDC 발급자 URL · 클라이언트 id(공개 · PKCE) · 규칙 `fdrl_` · 조직 UUID · 서비스 계정 `svac_` · (워크스페이스 = 기존 필드)} → `profiles.conf`에 `auth=wif` + `wif_*`(비밀 아님). [로그인…] = 스레드에서 OIDC 디스커버리 → `127.0.0.1:0` 리스너(리다이렉트 `http://localhost:<port>/callback`) → PKCE S256(OS 난수 · `nsql_vault::random_bytes`) → 기본 브라우저(`open_external` · 콘솔 창 없음) → 콜백(code · state 검증 · 대기 `ai.wif_login_timeout_secs` 180) → IdP 토큰 끝점 form POST(code+verifier) → **Anthropic `POST /v1/oauth/token`(JSON · `grant_type=jwt-bearer` · `assertion=id_token` · `federation_rule_id` · `organization_id` · `service_account_id` · `workspace_id` 조건부 · WIF 레퍼런스 11-11 확인)** → `{access_token sk-ant-oat01-…, expires_in}`. 토큰 묶음(refresh · id · access · 만료)은 볼트 봉투 `ai.<프로필>`에 JSON으로(키와 같은 봉투 · 로그·덤프 금지). 턴마다 API 스레드가 `ensure_access`: 만료 60초 전이면 **IdP refresh_token으로 새 ID 토큰** → 다시 교환(`jti` 1회용 규칙 — 저장된 id_token 재사용 금지) → 볼트 갱신. 요청 헤더 = `Authorization: Bearer`(`ApiAuth::Bearer` · 키는 `ApiAuth::Key`). [검증]도 같은 토큰으로 `/v1/models`. 한계 = CLI 모드·Codex에는 적용 안 함(CLI는 자체 로그인 · OpenAI 동등 기능 미확인) · 브라우저 로그인은 사용자 자리에서만(헤드리스 시험 불가 · 순수 함수만 단위 시험 = PKCE RFC 7636 벡터 · URL 인코딩 · 콜백 파싱 · 토큰 JSON · 교환 본문). | `ai/wif.rs`(새) · `claude_api::ApiAuth` · `host::ApiLaunch{auth,wif}` · `app/ai.rs ai_wif_login` · `view.rs` 프로필 폼 |

- 설정 키 추가 3 = `ai.prices` · `ai.paste_chip_chars` · `ai.wif_login_timeout_secs`(총 20) · i18n 추가 ≈ 20 · 시험 추가 = codex 1 · mod 2 · view 1 · wif 4.
- CI 교훈(40153854): 시험에서만 쓰는 래퍼는 bin clippy `dead_code` — 커밋 전 `scripts/check-3os.sh --quick`(clippy `--tests`만으로는 못 잡는다).
- 실기(T-475)에 추가할 것: ⑤ 프로필 [검증] 뒤 모델 콤보가 조회 목록으로 바뀌는지 · ⑥ 2,000자 이상 붙여넣기 = 칩 · ⑦ 조직 IdP가 있으면 WIF 프로필 로그인 → API 첫 턴(없으면 보류 · 설계만 검증됨).

### 15-6 사용자 실기 피드백 1차(2026-11-11 · 커밋 3f96c60e · 탭 분리/패널 재설계 커밋)
사용자 지시 4건(스크린샷 = 프로필 폼에서 `api#12` 저장 실패 · 참고 그림 = VS Code Claude 모델 선택 팝업).

| # | 지시 | 구현 |
|---|---|---|
| 1 | API 키 상자가 선택/입력이 안 됨 · 필수값 점검 | 원인 = 숨긴 WIF 상자의 옛 영역이 남아 같은 자리의 키 상자 대신 클릭을 가로챔("마지막 적중" 규칙) → 안 보이는 상자는 영역 0 · 첫 적중에서 멈춤 · Tab은 보이는 상자만. 점검 `AiView::form_check` = 이름 필수 + **볼트 비밀 이름 규칙**(영문·숫자·`_-.` · ≤61 · `.` 시작 금지 — `#` 거부) · 기본 URL은 http(s):// · 노력도는 5개 중 하나 · WIF 5필드 필수 · 새 프로필은 키 필수 → 첫 문제를 경고색으로 보이고 그 상자에 포커스. [저장]·[검증]·[로그인…] 모두 점검 뒤 진행. |
| 2 | 이름 변경 = rename · 복제는 따로 | 편집 폼은 원래 이름(`p_editing`)을 기억 → 저장 때 이름이 다르면 `ProfileStore::rename` + 볼트 비밀 옮김(get→put→delete) + 세션 참조(`Session.profile`) 갱신·저장. 새 이름이 이미 있으면 거부(`AiNameTaken`). 목록 행 [복제] = `<이름>-copy`(겹치면 -copy2…) + 비밀 복사 + 편집 폼 열기. |
| 3 | Claude 탭 · Codex 탭 분리 | 활동 막대 `view.ai`(Claude) + `view.ai_codex`(Codex · Material `terminal` 아이콘) · 팔레트 `Codex` 토글 · 패널·뷰는 하나를 **탭 제공자**(`AiState.tab`)로 나눠 쓴다: 제목 = 제공자 · 세션 목록 = 그 제공자 것만 · 프로필/모델 = 그 제공자 것 · 채팅 뷰 탭 키 = `ai.chat`(Claude) / `ai.codex`(Codex · 제목 Codex) · 탭을 떠날 때 보던 세션을 기억해 돌아오면 복원 · 편집기에서 다른 제공자의 채팅 탭을 앞으로 가져오면 패널 탭도 따라감(`ai_follow_view`). 패널 안 Claude|Codex 세그먼트 제거(`PanelAction::SetProvider` 삭제). **호스트 이벤트 라우팅** = 턴이 도는 세션 id(`AiState.run`)로 — 탭·세션을 바꿔도 답이 다른 대화에 섞이지 않는다(종전엔 "보이는 세션"에 붙였다). 메뉴 배타 비트 524288(종전 131072가 파일 검색과 겹쳐 함께 닫히던 충돌 수정). 설정 `ai.provider` = `ai.chat`/`ai.open`이 여는 기본 탭. |
| 4 | 공간 절약: 작은 아이콘 + 툴팁 · 선택은 큰 팝업 | 패널 위 한 줄 = 제목 + [⊕ 새 대화][🔑 프로필](16px 틴트 아이콘 · `IconBtn`) + [API/CLI] 칩(Claude · 누르면 전환) + Auto 스위치 — 전부 hover 툴팁(`draw_tooltip_in` · 도구줄 선례처럼 즉시). 아래 칩 두 개 [프로필 ▾][모델 · 노력도 ▾](CLI 모드는 모델 칩만) → `ai/picker.rs` **큰 선택 팝업**(참고 그림 그대로: 제목 · 두 줄 행 = 이름 굵게 + 설명 흐리게 · 현재 ✓ · 휠 스크롤 · 보이는 행 8 · 꼬리 행 = 노력도 점 5개 + Auto 스위치 · Esc/바깥 클릭 닫기 · ↑↓ Enter). 모델 설명 = 내장 표(opus/sonnet/haiku/fable · sol/sol-mini)는 짧은 설명 · 조회한 모델은 "제공자 목록에서". 프로필 설명 = 모델 · 키 상태/WIF. 세션 목록이 남은 높이를 다 쓰고 사용량 한 줄은 맨 아래(흐리게). 헤드리스 덤프에 `profiles=` · `picker=` 줄. |

| 5 | Codex도 CLI 기능(사용자 11-11 추가 지시) | Codex 패널에도 [API|CLI] 칩. API = API 키 프로필로 `codex app-server` 로그인(종전 그대로) · **CLI = codex CLI 자체(ChatGPT) 로그인을 app-server가 그대로 사용**(키 없음 · 프로필 칩 숨김 · 개인 사용 전용 = 110 §13-1 · 칩 툴팁에 안내). 엔진이 같으니 도구 16·승인 카드·`thread/resume` 이어하기 동일. 바닥 줄 = codex 경로 + `codex login status` 첫 줄(탭을 열거나 CLI로 바꿀 때 한 번 조회 · 스레드 · 콘솔 창 없음 · 이 PC = "Logged in using ChatGPT"). 설정 `ai.codex_cli_login`은 칩으로 대체해 **제거**(설정 파일의 옛 키는 unknown으로 보관되어 무해) · `ai.mode` 설명을 두 제공자용으로. |
- 시험 추가 = mod 1(이름 규칙·rename) · view 1(필수값) · picker 1(행 클릭·휠·점·스위치·닫기) · panel 1(팝업 열기·선택·툴팁 글) · 아이콘 ASCII 미리보기에 ai/codex/key 추가.
- 실기(T-475)에 추가: ⑧ 프로필 폼 키 입력·필수값 안내·rename·복제 ⑨ 활동 막대 Codex 탭 ↔ Claude 탭 전환(세션 목록·채팅 탭 분리) ⑩ 아이콘 툴팁 · 모델/프로필 큰 팝업(점으로 노력도 · Auto).

### 15-7 사용자 실기 피드백 2차(2026-11-11 · 커밋 06ddc1f4 · 프로필 간소화/자동 모델 이름 커밋)
실기 원문: 조직 범위 키(`sk-ant-usr-…`)로 [검증] → 400 "not scoped to a workspace" → "Default"(이름) 입력 → 400 "must be a valid workspace ID" → `wrkspc_…` 입력 → ✓ 모델 15개. 사용자 "프로필 관리가 너무 불편 · Model/Effort는 목록을 받아 콤보로 · 바뀌지 않고 버전까지 자동 반영되는 이름 · 저장된 키는 ●●●●로 보이게".

| # | 구현 |
|---|---|
| 1 | **폼 간소화**: 이름 · API 키(저장된 키는 placeholder `•••• (저장됨 — 비우면 유지)`) · 모델/노력도 **칩 → 큰 선택 팝업**(§15-6의 `Picker` 재사용 · 노력도 5개 설명 줄) · **[고급 ▸]**(기본 URL · Workspace id — 기본값과 다를 때만 자동 펼침) · 버튼 = **[저장 + 검증]** 하나(저장 → 키/토큰 있으면 바로 검증 → 모델 목록 → 폼에 반영) · 목록 **행 클릭 = 편집** · API 모드에서 프로필 없이 보내면 오류 대신 프로필 폼(작성 글은 보존). |
| 2 | **워크스페이스 자동 보정**: 검증이 "not scoped to a workspace"로 실패하고 Workspace id가 비어 있으면 `default`로 다시 시도 → 성공이면 프로필에 `default` 저장 + 안내("조직 키라 Default로 자동 설정 — 다른 워크스페이스는 고급에서"). 폼 점검에 Workspace id 형식(`wrkspc_…` 또는 `default` · 이름 거부) 추가. 06ddc1f4 = 그 오류 원문 대신 안내 + 칸 포커스 · 안내 줄 줄 바꿈. |
| 3 | **자동 모델 이름(가족)**: `opus` · `sonnet` · `haiku` · `fable`(Claude) · `sol` · `sol-mini`(Codex). 세션/프로필/설정의 모델이 가족 이름이면 **보낼 때** `resolve_model`이 프로필이 조회한 목록(없으면 내장 표)에서 **버전 숫자가 가장 큰 id**(같으면 날짜 없는 짧은 별칭)로 푼다 — 다음 판이 나오면 [검증] 한 번으로 자동 반영. 모델 팝업 = 가족 이름 먼저(설명 "자동: 최신 opus(지금 claude-opus-5-5)") → 조회/내장 id. 기본값 `ai.model=opus` · `ai.codex_model=sol`. 비용 추정도 풀어서. |
| 4 | 상태줄 "Clipboard unavailable (Linux: …)"가 Windows에 뜨던 문구 → "클립보드에 글이 없습니다(Linux는 …도 확인)". |

- 시험 = mod(가족 풀기 · 항목 · 노력도) · view(필수값에 Workspace 형식) 갱신.
- 실기(T-475)에 추가: ⑫ 프로필 폼 새 흐름(이름+키 → [저장 + 검증] → 자동 보정 안내 · 모델 팝업에서 `opus` 선택 → 보내기).

### 15-8 사용자 실기 피드백 3차(2026-11-11 · 커밋 = 이 절의 구현 커밋)
| # | 실기 원문 | 구현 |
|---|---|---|
| 1 | CLI 모드에서 "Not logged in · Please run /login" — "이미 API로 연결했는데 뭐가 더 필요해" | CLI 모드는 프로필 키를 쓰지 않는다(claude 명령 자체 로그인). ① 그 오류 뒤 안내 줄 추가("칩을 API로 · `claude login` 한 번 · 또는 설정 `ai.cli_key = profile`") ② 새 설정 **`ai.cli_key`**(login 기본 · profile = 현재 프로필 키를 `ANTHROPIC_API_KEY` 환경 변수로만 CLI에 전달 · API 과금) ③ 채팅 머리 줄의 모드 글자 = "CLI(claude 로그인)" / "CLI(codex 로그인)". |
| 2 | 세션 이름 바꾸기에서 한글 입력 안 됨 | 우클릭 메뉴 → [이름 바꾸기]는 `route_ai_menu` 경로라 `ime_refresh()`가 빠져 IME가 꺼진 채였다 → 한 줄 추가. |
| 3 | 승인 카드 버튼이 아래로 가려짐 | 대본 높이 합계에 대기 중 승인 카드의 버튼 줄(btn_h+6)이 빠져 맨 아래 스크롤이 모자랐다 → 포함. |
| 4 | 작성 상자 Shift+Enter 최대 8줄 · 설정 가능 | `ai.composer_max_lines`(기본 8 · 2~40) · 그 뒤로는 상자 안 스크롤. |
| 5 | Claude · Codex 설정을 따로 | 제공자별 키: Claude = `ai.mode`/`ai.model`/`ai.effort`/`ai.auto` · Codex = `ai.codex_mode`/`ai.codex_model`/`ai.codex_effort`/`ai.codex_auto`(패널 칩·팝업이 탭의 키에 저장 · 새 세션도 탭의 키로) · 설정 창 라벨에 "Claude:"/"Codex:" 접두(Claude CLI 전용 = "Claude CLI:"). 공통 = 비용 표 · 행 상한 · 시간 · 토큰 · 이력 · 붙여넣기 칩 · 작성 상자 줄 수 · MCP 포트 · WIF 대기. |
| 6 | 입력란에서 휠을 굴리면 뒤의 대본이 스크롤 | 뷰가 마지막 마우스 위치를 기억해 작성 상자 위면 상자 안 스크롤 · 아니면 대본. |
- 설정 키 = +5(ai.cli_key · ai.codex_mode · ai.codex_effort · ai.codex_auto · ai.composer_max_lines) → 합계 24.

### 15-9 사용자 실기 피드백 4차(2026-11-11 · §15-8과 같은 커밋 + 첨부/하단 바 커밋)
| # | 실기 원문 | 구현 |
|---|---|---|
| 1 | 터미널 `claude`는 로그인돼 있는데 앱 CLI 모드는 "Not logged in" | 원인 = `--bare`(`ai.bare` 기본 켬)가 Claude Code의 자체 로그인도 건너뜀 → `--bare`는 프로필 키를 CLI에 넘길 때(`ai.cli_key = profile`)만 붙인다 · 설정 설명 수정. |
| 2 | 코드 블록 [복사][편집기에 넣기]를 아이콘+툴팁으로 · [실행] 버튼으로 바로 아래 결과 | 상자 머리 = 아이콘 3개(복사 · 편집기 · ▶ 실행 · 툴팁). [실행] = 사용자가 보이는 SQL을 직접 실행하는 것과 같다(`gate_open()` → `run_text_whole` · DR-34) → 결과는 **그 블록 아래 꼬리 줄**(조회 = 앞 8행 TSV · 그 밖 = 상태 줄 · 실패 = 경고색 · 12줄/200자 상한 · 화면에만 · AI로 안 보냄 · 이력 저장 안 함). `ChatMsg.block_results` · 뷰 캐시 키에 결과 길이 포함. |
| 3 | 프롬프트를 넣으면 진행 중인지 모른다 — 토큰 수·시작·경과 | 상태 줄 = "진행 중 · 시작 HH:MM:SS · N s · 받은 글자 M"(TextDelta마다 + 이벤트 루프 1초 틱 `ai_tick_due`) · 끝나면 종전대로 "stop · ms". |
| 4 | 클립보드·파일 첨부를 상단 칩으로 관리·미리보기 | 칩 = [본문(클릭 = 미리보기)][×] · 이미지 칩 = 이름 W×H(PNG/GIF/JPEG 머리 파싱 `image_dims`) · 미리보기 오버레이(PNG = 그림 · 비율 유지 · 그 밖 = 안내/앞 12줄 · 클릭/Esc 닫기) · PNG 디코더 `png::decode`(8비트 · 비인터레이스 · zlib = nsql-drivers ext `inflate_zlib` · miniz_oxide 재사용). Ctrl+V = 글이 없으면 클립보드 이미지 칩. |
| 5 | 하단 줄을 [+]·네모 안 아이콘으로 간략히 · 최상단 상태 줄 제거 · 에이전트·모델·노력도 간단 표시 | 머리 줄 제거. 아래 줄 = [+](첨부 메뉴 = 파일 · 클립보드 · 결과 그리드) [🔑] [/](명령 · 툴팁 = 키 안내) 상태 글 … [에이전트 칩 = Claude · API · 프로필] [모델 · 노력도 칩] [Auto 스위치] [➤/■]. 키 안내 줄 제거(툴팁으로). |
- 설정 키 = §15-8 포함 합계 24 그대로(이 절은 새 키 없음).
- 다음 = ③ 편집기 옆 분할 Claude/Codex(미니맵 오른쪽) + AI 툴바 그룹(새 Claude 탭 · 옆 표시 토글 · 패널) · 설정 `ai.side_width`.

### 15-10 편집기 옆 분할 대화 창 + AI 툴바 그룹(2026-11-11 · 사용자 "미니맵 옆에 스플릿된 Claude · AI 툴바 그룹")
- **분할 창**: 설정 `ai.side`(끔 기본)가 켜지면 레이아웃이 편집기 폭에서 `ai.side_width`(기본 420 · 240~1200 · 편집기 폭의 절반 상한)를 떼어 **편집기 오른쪽(미니맵 옆)** 에 `AiState.side_rect`를 잡고, 그 자리에 채팅 뷰(`AiView` = 탭과 같은 부품)를 그린다. 뷰 탭(채팅 탭·확장 상세 등)이 활성인 동안은 숨김(편집기 자리 전체). 결과 영역은 그대로 전체 폭.
- **포커스**: 분할 창 안을 누르면 `side_focus = true`(키·IME·붙여넣기·Ctrl+V 이미지가 창으로) · 밖을 누르면 편집기로. `Focus` 열거는 늘리지 않았다(`Focus::Editor` + 깃발).
- **AI 툴바 그룹** `ai`(기본 `run`·`conn` 뒤 · `view` 앞 · 숨김/순서는 종전 툴바 편집 규칙): [대화 탭](`ai.tab` = 패널 현재 탭 제공자의 채팅 탭 열기) · [편집기 옆에](`ai.side` 토글 · 체크 표시). 팔레트 = "AI: 대화 탭" · "AI: 편집기 옆에".
- 설정 키 +2(`ai.side` · `ai.side_width`) → 합계 26.
- 한계·다음: 분할 창 폭 드래그 스플리터는 없음(설정으로) · 분할 창과 채팅 탭은 같은 세션을 보인다(동시에 다른 세션 두 개는 2차).

### 15-11 사용자 실기 피드백 5차(2026-11-11 · 분할 창 = 탭 안쪽 · 탭별 세션 · 드래그 선택 · CLI MCP)
| # | 실기 원문 | 구현 |
|---|---|---|
| 1 | 우측 Claude가 탭 그룹(탭 줄)까지 차지 → 탭 안쪽 우측에 | `Editors.side_reserve`(본문 오른쪽 예약 폭 · 탭 줄은 전체 폭 · 본문만 좁힘) · `side_rect` = 본문 오른쪽(`body_rect()` 기준). |
| 2 | 각 탭마다 AI 보기 · 쓰던 세션이 있으면 연결 · 없으면 새 세션 | `AiState.side_tab`(보는 편집기 탭 id) · `tab_sessions`(탭 id → 세션 id). 그리기 직전 `ai_side_prepare`가 탭 변화를 감지해 그 탭의 세션으로(제공자가 다르면 탭 전환 동반) · 없으면 새 세션 생성·기록. 턴이 도는 동안은 바꾸지 않는다. |
| 3 | 분할 창 상단에 이름 + (VS Code처럼) 세션 선택 · 새 세션 · Claude/Codex 선택 | 분할 창 모드 머리 줄 = 세션 제목 + [≡ 세션][+ 새 세션](툴팁). [≡] = 큰 선택 팝업(§15-6 `Picker`): "＋ 새 세션(Claude)" · "＋ 새 세션(Codex)" · 이전 세션들(설명 = 제공자). 고르면 그 편집기 탭에 연결. |
| 4 | 대본에서 드래그로 글 선택·복사 | **줄 단위 선택**: 대본 안 누른 뒤 끌면 줄 범위 강조(accent 18%) · Ctrl+C(편집 메뉴 복사 경로의 폴백 = 작성 상자 선택이 없으면 대본 선택 줄) · Esc/클릭 = 해제. 글자 단위 선택은 2차. |
| 5 | CLI 모드 "Connection 정보 조회"에 이상한 답(ToolSearch · `mcp__nexa__approve` not found · 사용자 PC MCP 서버 목록) | `--bare`를 빼자 Claude Code가 사용자 설정의 MCP 서버(Claude_Docs · mcp-genagent)를 함께 올리고 앱의 `nexa` 서버가 빠졌다 → `--strict-mcp-config`(앱이 넘긴 `--mcp-config`만) 추가. |

### 15-12 스킬(앱 계층 플레이북) · 세션이 쓸 수 있는 모델 · 모델 칩 팝업(2026-11-11 · 사용자 지시 3건)
- **스킬**(사용자 "MCP로 줄 정보·기능은 skills로 따로 정리"): 모델이 "지금 접속 정보"를 DBMS(v$session)로 바로 묻던 것 → `ai/skills.rs`에 플레이북 5개(`connections` · `metadata` · `editor` · `results` · `app` · 영어 · 모델용). 시스템 프롬프트에는 **색인**(이름 — 언제) + 규칙("맞는 스킬을 `skill` 도구로 읽고 앱 도구를 DBMS보다 먼저")만 · 본문은 새 읽기 도구 **`skill(name)`** 으로(API · CLI(`--append-system-prompt`) · Codex 공통 · 프롬프트 짧게). 도구 17종.
- **`session_info` 확장** = 앱 계층: 현재 편집기 세션(프로필 · 방언 · 접속 · 스키마 · 현재 DB · busy · 미커밋 수 · 가린 설명) + 세션 관리자와 같은 **전체 세션 목록**(공유/전용(주인 탭) · 접속/끊김/busy · 유휴 초 · 미커밋). DBMS 질의 없음.
- **세션이 쓸 수 있는 모델만**(사용자 "Free에서 안 되는 모델이 기본" · "현재 세션이 쓸 수 있는 것 중 선택" · "CLI도 일반 기본값 + 선택 가능"): Codex CLI = 로그인 상태 조회 스레드가 `model/list`(키 없이 = CLI 로그인)도 받아 `AiState.codex_cli_models`에 → 모델 팝업 선택지 · 자동 이름 `sol` 풀기의 기준. 목록에 없는 모델이 지정돼 있으면 **계정 기본 모델로 바꿔 보내고 Note로 안내**(`AiModelSwapped`). API 모드 = 프로필이 [검증] 때 받은 목록(종전). Claude CLI = 목록 API가 없어 가족 이름 + 내장 표(CLI가 별칭을 받는다). `account/updated` 알림은 대본에 안 올림.
- **모델 칩 = 버튼**(사용자 "클릭해도 변경이 없어"): 하단 [모델 · 노력도] 칩 → 큰 팝업(이 세션의 모델 선택지 + 노력도 점 5 + Auto 스위치) → `/model` · `/effort` · `/auto` 명령으로 세션에 적용.

### 15-12-a AI 채팅 탭 = 독립 단위(2026-11-14 · 사용자 결정 · f18b7a68)

- 별도 탭의 AI 채팅(뷰 키 `ai.chat*`·`ai.codex*`) = **독립 단위** — 탭에 세션 표식(편집기 탭과 같은 표식 메뉴 · `extensions.tab_badge`와 무관) · 자기 연결(표식 메뉴로 바꾸면 그 탭의 run_query가 그 연결로) · 자기 변수 표(변수 창 제목 = 채팅 탭 이름).
- 편집기 옆 분할 창 = 그 편집기 탭에 종속(종전).
- 독립 탭에서 편집기가 필요한 기능 = 흐림·거절: 코드 블록 ▦ · 편집기에 넣기 · 교체 · 줄 번호 링크 · 첨부 "결과 그리드" · `/grid` → AI 패널 상태 글 `AiNeedsEditorTab` · ▶·복사·줄바꿈은 그대로 · ▶·▦ 코드 블록에 클라이언트 명령/여러 문장이 있으면 문장 단위(`run_text`).
- 협업 판정 ✓([journal 11-14 §2](journal/2026-11-14.md)).

### 15-13 2차로 미뤄 둔 셋 = 글자 단위 선택 · 분할 창 스플리터 · 채팅 탭/분할 창 세션 분리(2026-11-11 저녁 · 사용자 결정 3건)
사용자 "완료되지 않은 진행사항 있으면 바로 개발" → 미완은 없고 §15-10·§15-11이 2차로 적어 둔 세 가지를 질문 창으로 물어 전부 "지금 개발".

| # | 결정 | 구현 |
|---|---|---|
| 1 | 대본 드래그 선택 = **글자 단위**(종전 줄 단위) | `AiView.sel = ((줄, 글자), (줄, 글자))` 앵커·초점. 누른/끄는 점은 `(평면 줄, x px)`로 두고(`drag_from`·`drag_to`) **다음 그리기**에서 글자 자리로 푼다(`resolve_col` = 줄 종류별 글 x(코드 = `code_x` · 제목/역할 = 굵은 글꼴) + 접두 폭 이진 탐색 `col_at` · 반 넘으면 다음 경계). 강조 = 글 **위에** 반투명(accent 25% · 첫/끝 줄은 글자 경계 · 중간 줄은 끝까지 + 줄 바꿈 표시 여백) — 종전엔 코드 상자 배경이 강조를 덮었다. `selection_text` = 첫 줄 꼬리 + 중간 통째 + 끝 줄 머리(거꾸로 끌어도 같다 · 시험 `char_selection_resolves_and_copies`). 덤프 첫 줄에 `sel=`. |
| 2 | 분할 창 폭 **스플리터** | `App.split_ai`(nexa-ctl `Splitter` 세로 · 띠 = 본문과 분할 창 사이 틈 `grip` · 분할 창이 보일 때만 rect) · 끌면 `ai.side_width` = (분할 창 오른쪽 − (띠 x + grip)) / 배율 → 240~1200 클램프 → `layout()` · 놓으면 설정 저장. 커서 ColResize · tick · 드래그 중 우선 라우팅은 기존 스플리터 셋과 같은 자리. 편집기 폭 절반 상한(레이아웃)은 그대로. |
| 3 | 채팅 탭 세션 ↔ 분할 창 탭별 세션 **분리** | `AiState.chat_claude/chat_codex`(제공자별 채팅 탭 세션 id) · `on_chat: Option<Provider>`(마지막으로 그린 면). 채팅 탭 그리기 직전 `ai_chat_prepare` = 분할 창/다른 제공자 탭에서 넘어왔으면(`on_chat != Some(tab)`) `ai_chat_restore`(채팅 탭 세션으로 · 턴 중엔 안 바꿈) → `ai_chat_mark`(지금 세션 = 채팅 탭 세션 · `on_chat`). 분할 창 `ai_side_prepare` = `on_chat`을 비우고 채팅 탭에서 넘어왔으면 탭 세션을 다시 잡는다(이때 떠나는 탭 기록은 하지 않는다 · 채팅 세션을 엉뚱한 탭에 묶지 않게) · 매 그리기에 `tab_sessions[tab] = cur`(종전엔 탭을 바꿀 때만 기록해 분할 창 [+] 새 세션이 탭에 안 묶이던 틈 수정). 활동 막대로 채팅 탭을 열 때(`ai_toggle_panel`)도 채팅 탭 세션 복원 · `ai_open_chat`(세션 선택·새 세션 뒤)은 그 세션을 채팅 탭 세션으로 기록. |

- 설정 키 새로 없음(합계 26) · i18n 새로 없음 · 시험 +1(view) · `ai::` 31/0 · clippy `--tests -D warnings` ✓.
- 한계: 작성 상자 초안은 면(채팅 탭/분할 창)끼리 공유(뷰 부품 하나) · 분할 창과 채팅 탭은 동시에 보이지 않는다(채팅 탭 = 뷰 탭이라 분할 창 숨김 · §15-10 그대로).

### 15-14 실기 피드백 6차(2026-11-11 저녁 · 스크롤바 설정 · 클립보드 칩 · 첨부 · `/status` `/account` · 프로필 목록/상세 · 그리드 버튼 · 직접 접속)
| # | 실기 원문 | 구현 |
|---|---|---|
| 1 | 스크롤바 기본 표시 방식 설정이 있는지 · 작성 상자 스크롤바가 항상 보임 | 설정은 없었다(nexa-ctl에 전역 자동 숨김 지연만 · 기본 2초) → 새 키 **`ui.scrollbars`**(auto = 스크롤 때만(기본) · always = 항상 · 모양 분류 · `apply_scrollbars` = `nexa_ctl::controls::scroll::set_hide_delay_ms(0 또는 기본)` 한 곳 · 모든 `ScrollBars`가 그때그때 읽는다 · 시작 때 + 설정 변경 때). 항상 보이던 **원인** = AI 뷰(작성 상자·프로필 상자·대본)의 스크롤바가 이벤트 루프 틱을 못 받아 자동 숨김이 진행되지 않았다 → `AiView::tick` 추가 + 루프 틱에 연결. |
| 2 | 클립보드 이미지 칩 = `clipboard.png`(형식 확장자) + 가로세로 · 용량은 빼고 · 미리보기 아래에 크기 | 라벨 `clipboard.png`(클립보드는 항상 PNG) · 칩 = 이름 W×H(치수 모르면 이름만) · 미리보기 둘째 줄 = `W×H · image/png · N KB`. |
| 3 | 파일을 골랐는데 첨부가 안 됨(스크린샷 = "Not sent: … password or API key") | **원인** = 글 파일의 비밀 패턴(`CONNECT user/pw@db` · `password=` · API 키)을 `contains_secret`이 거부(협업이 redact 셋째 규칙으로 짚음) — 오류 줄 하나뿐이라 "안 된다"로 보였다. **바꿈** = 거부 대신 **가리고 첨부**(`ai::redact_count` = `******` + 가린 곳 수 · 파일·붙여넣기 칩·클립보드 글 세 경로 공통) + Note·상태줄 "비밀 N곳을 가리고 첨부"(새 키 **`ai.secret_warn`** 기본 on · off = 조용히 가림 · 가림 자체는 끌 수 없음 · 사용자 "경고는 하되 첨부는 되게 · 경고 끄기 가능") · 512 KB 상한은 상태줄에도. 작성 상자에 직접 친 비밀은 종전대로 보내지 않는다. V1 = 기동 명령 **`ai.attach:<경로>`**(파일 창 단계 건너뜀) · `NSQL_TRACE_AI=1` 흔적 `[ai] attach <경로> -> attachments=N`. |
| 4 | `/` 명령이 전부인가 · 상태·계정 정보는 못 보나 | **`/status`** = 세션(제목·id) · 제공자/모드/프로필 · 모델(자동 이름 → 실제 id) · 노력도 · Auto · 메시지/첨부/행 데이터 · 사용량(입력·출력·캐시·$) · 턴(대기 / 진행 중 시작·경과). **`/account`** = 현재 API 프로필(인증 = 키 저장됨(••••)/없음/WIF 발급자 · 엔드포인트 · 워크스페이스 · 검증 때 받은 모델) · Claude CLI 키 출처(`ai.cli_key`)·프로그램 · Codex CLI 로그인 줄 + 계정 모델. 전부 앱이 아는 것만(네트워크 0 · 비밀 없음). |
| 5 | Alt(전체 보기) 때 글이 버튼을 덮음 · 아이콘 버튼 · 목록/편집 분리 · [+] 별도 · [←]로만 복귀 | `Page::Profiles`(목록) / **`Page::ProfileEdit`**(상세) 분리. 목록 = 제목 + [+](새 프로필) · 행 = 이름 ✓ [WIF] · 모델 · 키 상태(**행 글은 버튼 앞 자기 영역으로 클립** → Alt 전체 보기도 버튼을 덮지 않는다 · 전체 글은 **Alt + hover 말풍선**(URL 포함)) · 행 오른쪽 아이콘 [✓ 사용][✎ 편집][⧉ 복제][✕ 삭제](툴팁) · 행 클릭 = 상세 · [대화로]. 상세 = [←](목록으로 · 저장 안 한 것은 버림) + 제목 + 종전 폼 · [저장 + 검증] 뒤에도 상세에 머문다 · Esc = 목록 · 목록 Esc = 대화. 패널 [🔑]·`/profiles` = 목록으로(종전엔 바로 폼). |
| 6 | 코드 블록 [실행]은 글 요약 — 하단 그리드에 바로 보는 버튼 | 아이콘 [▦ 그리드에서 결과 보기](맨 오른쪽 · `mi_table`) = 보통 실행과 같은 길(`gate_open` → `run_text_whole`) → 결과 그리드 · 대본엔 안 붙임. |
| 7 | 비밀번호 저장된 프로필은 `connect`가 직접 접속 · 없으면 알려 주고 직접 로그인 안내 | `connect` 도구 = 볼트 스펙에 비밀번호가 있으면 로그인 목록 [Login]과 같은 길(`login_profile` · 창 없음 · 비밀번호는 호스트 안)로 바로 접속하고 "session_info로 확인하라"고 돌려준다 · 없으면 접속 창을 열고 "사용자가 직접 비밀번호를 넣고 확인해야 한다"고 돌려준다 · 모르는 이름 = 오류(list_profiles 안내). 도구 설명·`connections` 스킬 3단계 갱신. 승인은 종전대로(쓰기 도구). |

- 설정 키 +2(`ui.scrollbars` · `ai.secret_warn` → AI 키 27) · i18n +16 · 시험 그대로(ai:: 31).
- 한계: 파일 첨부 실패는 재현 전 — 사용자 답(파일 종류·크기·오류 줄)을 받아 판정.

### 15-15 실기 피드백 7차(2026-11-11 저녁 · 캐럿 포커스 · 탭별 분할 창 · 활동 막대 아이콘 · 툴바 툴팁)
| # | 실기 원문 | 구현 |
|---|---|---|
| 1 | 편집기·프롬프트 입력기 등 **실제 포커스가 있는 곳만 캐럿이 깜빡**이고 나머지는 빔만 — 전체 입력창 포커스 확인 로직 | (스크린샷 = 편집기와 분할 창 작성 상자가 동시에 캐럿) **nexa-ctl `TextBox::set_idle_caret`**(새 플래그 · 포커스 없으면 흐린 색 **고정 빔** · 포커스 있으면 종전대로 호스트 위상 `caret_on`으로 깜빡) — 편집기 탭(`make_box`)·AI 작성 상자에만 켬(검색 상자 등 한 줄 상자는 종전처럼 포커스 때만). **포커스 판정 한 곳**: `set_focus` = 편집기 TextBox의 focused = `Focus::Editor && !side_focus && !채팅 탭` · 분할 창 안/밖 클릭으로 `side_focus`가 바뀌면 작성 상자·편집기 focused를 함께 맞춘다 · 분할 창 끄면 편집기로. AI 뷰(채팅 탭·분할 창·확장 상세)의 `RasterCtx`에도 `with_caret_on` 주입(종전엔 항상 켜진 캐럿). |
| 2 | 좌측 활동 막대 Claude·Codex 아이콘 변경 | Claude = 여덟 꼭지 별 하나(별표 꼴 · 바깥 400 · 안 150 · 한 다각형) · Codex = Material `code`(`</>`). `mi_act_ai` · `mi_act_codex`. |
| 3 | 툴바 AI 버튼(✦ · ▮)에 툴팁이 없음 | 코드상 `ToolItem::tip(TipAiTab/TipAiSide)`가 있고 `TOOLBAR_TIPS` 표에도 있다 → 원인 미확인 · **V1**(hover 캡처)로 판정 뒤 수정. |
| 4 | A 탭에서 분할 창을 켜고 B 탭으로 가면 B에는 안 보이게 — 탭마다 직접 켜야 적용 | `AiState.side_tabs`(켠 편집기 탭 id 집합) · `ai_side_on()` = 활성 탭이 집합에 있나 → 레이아웃·툴바 체크·메뉴 체크 전부 이것 · 토글은 활성 탭만 · 설정 `ai.side` = 마지막 토글 값(시작 때 활성 탭에만 적용) · 탭을 바꿔 켬/끔이 달라지면 그리기 직전에 `layout()`. 탭별 세션(`tab_sessions`)은 그대로. |

- 설정 키 그대로(`ai.side` 뜻만 "마지막 토글 · 시작 때 활성 탭") · i18n 없음 · nexa-ui 커밋 1(nexa-ctl 시험 76/0).

### 15-16 실기 피드백 8차(2026-11-11 밤 · 첨부 칩 여러 줄 · 툴바 툴팁 원인)
| # | 실기 원문 | 구현 |
|---|---|---|
| 1 | 6개를 첨부했는데 칩이 2개만 보임 | 칩이 한 줄을 넘으면 숨기던 구조 → **여러 줄로 감쌈**: 그리기 전에 칩 폭을 재서 (번호 · 줄 · x)를 계산하고 `chips_h = 줄 수 × (btn_h + 6)` → 작성 상자가 그만큼 아래로. 클릭(미리보기)·[×] 사각형도 같은 배치. |
| 2 | 툴바 **모든** 버튼의 툴팁이 안 보임(탐색기 툴팁은 보임) | **원인** = 10-07 설정 `ui.tooltip_above`(기본 켬)로 전역 툴팁 방향이 "기준 위"가 된 뒤, 맨 위 툴바의 툴팁이 **메뉴바 띠**에 그려지고 최상위 메뉴바 패스(`paint_seg_overlays` 끝)가 메뉴바 바탕을 다시 칠해 덮었다(탐색기는 위에 아무것도 없어 보였다 · 헤드리스 캡처에서도 전부 없던 이유). **수정(사용자 결정 A · 11-11 "아래 표시는 불편")** = **그리기 순서**: 툴바 툴팁을 메뉴바 패스 **뒤**(진짜 최상위)에 그린다(`paint_seg_overlays` 끝) · 메뉴가 열린 동안은 안 그림 · 전역 "위" 선호 그대로(툴팁이 메뉴바 띠 위에 잠깐 겹침). 먼저 push했던 B안(nexa-ui 2d86926 = 툴바만 아래 방향)은 되돌림(revert). |

| 3 | 대본 드래그 선택 때 자동 스크롤이 없음 | 끄는 중 포인터가 대본 위/아래 밖이면 `AiView::tick`이 줄 높이 2배씩 그쪽으로 넘기고 가장자리 줄을 초점으로(`clamp_to_transcript`) · 이동/놓기도 밖이면 가장자리 줄. |
| 4 | 모델이 "전용(private) 세션은 못 만든다"고 답함 — 사용자 "`CONNECT LNK_PRD;` 명령만 실행되면 되는데 따로 API가 필요한가" | 필요 없다. `connect` 도구에 **`scope=private`**: 저장 비밀번호가 있으면 활성 편집기 탭에서 `CONNECT <profile>;`를 보통 실행과 같은 길(`run_text_whole` → run.rs CONNECT = 그 탭 전용 세션)로 실행 · 없으면 오류로 안내(공유 창 또는 사용자가 직접). `connections` 스킬에 "전용 세션도 만들 수 있다 · 못 한다고 말하지 말 것" 명시. |
| 5 | (협업 관찰) 툴팁에 단축키가 두 번 "전체 실행(F5) (F5)" | 한국어 글 3개(문장 실행 · 전체 실행 · 로그 창)에 괄호 앞 공백 → `without_shortcut`이 뗀다. |

- nexa-ui = 2d86926 revert(변경 없음) · nexa-sql = view.rs 칩 배치·자동 스크롤 + paint.rs 툴팁 순서 + connect scope · i18n 글 3.

### 15-17 객체 검색 하네스(2026-11-11 밤 · 사용자 "테이블을 검색할 때 이미 읽힌 Meta로 더 빠르게 · Skills로(Codex도)")
- **원칙** = 탐색기 메타 스냅샷(메모리 · `nsql_run::meta::Snapshot`)만 본다 — DBMS 질의 0 · 새 적재 없음(지연 로딩 그대로 · 61 §1-8).
- **`ai/search.rs`**(순수 · 시험 2): `name_score(q, name)` = 정확 1000 > 접두 800− > 낱말 머리(`_` 뒤) 600− > 부분 400− > 퍼지(`palette::fuzzy_score_lower`) <200 · `parse_query` = `SCHEMA.NAME` 스키마 떼기 + 띄어쓰기 AND 낱말 · `words_score` = 전 낱말 합.
- **`search_objects`** 갱신: 점수 순(현재 스키마 +50) · `schema` 필터 · `kind` 필터 · **`in_columns`** = 읽힌 컬럼 이름도(객체 점수 −300 · 행 = `schema.table.column<TAB>column<TAB>type<TAB>of kind`) · 행에 `cols=loaded(N)|-` · 첫 줄 요약(적중 수 · 읽힌 객체 수 · 스키마 수 · 현재 스키마).
- **새 도구 `meta_stats`**(읽기): 스키마별 종류별 객체 수 · 컬럼 읽힌 객체 수 · 현재 스키마 — 모델이 "앱이 아는 범위"를 보고 사전 질의가 필요한지 판단. 도구 18종.
- **`metadata` 스킬** = 하네스 순서(①`meta_stats` → ②`search_objects`(낱말 AND · `SCHEMA.NAME` · kind · in_columns) → ③`describe_object`(컬럼 안 읽혔으면 탐색기 펼치기 또는 `SELECT * … WHERE 1=0`) → ④`object_source` → ⑤ 그때만 사전 질의) · **Claude·Codex 공통**(같은 도구 · Codex는 시스템 프롬프트를 첫 턴 `<app_context>`로 받으므로 색인·`skill` 도구 동일).
- **`/skills`**(사용자 11-11): 색인(이름 — 언제) + 안내 · `/skills <이름>` = 플레이북 본문(모델이 받는 영어 글 그대로) · 모르는 이름 = 목록 안내. 헤드리스 `ai.send:/…`는 슬래시로 푼다(협업 관찰).
- MCP 구성 요약(사용자 질문 11-11): API 모드 = 요청 `tools`에 18종 직접 · Claude CLI = `--mcp-config`의 `nexa` 서버 하나(`nsql mcp-bridge` stdio ↔ 앱 TCP `ai.mcp_port`) + `--strict-mcp-config` + `--permission-prompt-tool mcp__nexa__approve` + `--append-system-prompt` · Codex = app-server `dynamicTools`(MCP 아님) · 스킬 본문은 `skill(name)`.

- ★ **메타 우선(11-14 · 2872aa63)** = `ai::guard::is_catalog_query(sql, dialect)`(데이터 사전만 읽는 조회 · 보수적) · `ai.meta_first`(기본 on) = 그 턴에 search_objects/describe_object/meta_stats 없이 카탈로그 run_query를 하면 한 번 "not executed: this reads the data dictionary…"로 되돌림(턴당 1회) · `ai.meta_auto`(기본 on) = 카탈로그 조회는 행 데이터 동의 카드 없이(`rows_allowed`는 그대로 · 사용자 표·조인은 종전대로 동의) · search_objects = 표·컬럼 코멘트도 검색(비ASCII 낱말 = 컬럼 코멘트까지) · 요약 줄 "comments loaded for: …" · 코멘트 미적재 스키마 = 지연 적재 + hint · describe_object 컬럼 줄 끝 `-- 코멘트` · 협업 판정 ✓([journal 11-14 §3](journal/2026-11-14.md)).

- ★ **describe_object 컬럼 직접 적재 · 빈 결과 탐침(11-14 · 8b82af2a)** = 컬럼이 안 읽힌 객체는 앱이 직접 상세·컬럼을 청하고(`wait_meta`) 읽히거나 8초 뒤 답 — 모델에게 "WHERE 1=0으로 적재" 안내는 뺌 · `SELECT … WHERE 1=0|1=2|0=1`(맨 바깥 · 집합 연산·사용자 루틴 없음) = `ai.meta_auto` 아래 동의 카드 없이 · 협업 판정 ✓([journal 11-14 §4](journal/2026-11-14.md)).

## 16. 연계 구조 — MCP · hooks · skills · agents(2026-11-11 밤 · 사용자 "연계에 필요한 기능을 구성·개발하고 유기적 연계를 문서로")

### 16-1 네 개념과 공급자 추상(사용자 "용어는 Claude식 · Codex 등 다른 AI도 같은 기능을 추상화 + 구현부")
| 개념 | 뜻(앱 쪽 원천 · 공급자 무관) | Claude API | Claude CLI | Codex(app-server) |
|---|---|---|---|---|
| **도구**(MCP) | 앱 기능을 모델이 부르는 함수 = `ai/tools.rs` `TOOLS`(24종) — 접속·세션·메타·편집기·결과·설정·명령·탭·파일·확장·스킬 | 요청 `tools` · `tool_use` | MCP 서버 **`nexa-sql`**(`nsql mcp-bridge` stdio ↔ 앱 TCP `ai.mcp_port`) · 이름 `mcp__nexa-sql__<도구>` · `--strict-mcp-config` | `thread/start.dynamicTools` · `item/tool/call` |
| **승인** | 쓰기 도구·행 데이터·비밀 = 호스트 `ai_tool_request`(끌 수 없음) | 앱 카드 | `--permission-prompt-tool mcp__nexa-sql__approve` | tool/call 응답 전 카드 |
| **스킬** | 요청 종류별 플레이북 = `ai/skills.rs`(색인은 시스템 프롬프트 · 본문은 `skill` 도구 · `/skills`) | 시스템 프롬프트 | `--append-system-prompt` | 첫 턴 `<app_context>` |
| **훅** | 모델 없이 결정적으로 끼는 단계 = `ai/hooks.rs`(턴 끝: ```sql 블록 앱 포맷 · 빈 줄 정리 · 설정 `ai.hook_*`) + 호스트의 전 도구 훅(승인·가림·동의) | 공통 | 공통 | 공통 |
| **에이전트** | 세션의 역할 프리셋 = `ai/agents.rs`(지시 + 허용 도구 · `/agent` · 호스트가 허용 목록 강제) | 시스템 프롬프트 + 강제 | 같음 | 같음 |

대응 = `ai/bridge.rs`(`MCP_SERVER` · `tool_short_name` · `approve_tool_name` · `Delivery::describe` → `/status` "연계 방식" 줄). 새 공급자 = bridge 한 줄 + 런처/파서(host.rs)만.

### 16-2 한 요청이 흐르는 순서
1. 사용자 글(+첨부 · 비밀 가림 훅) → 세션의 **에이전트** 지시 + **스킬 색인**이 든 시스템 프롬프트와 함께 공급자로.
2. 모델이 스킬에 맞으면 `skill(name)`으로 플레이북을 읽고 **도구**를 부른다 → 호스트 전 도구 훅(에이전트 허용 · 쓰기 승인 카드 · 행 데이터 동의 · 비밀 가림) → 앱이 실행(메타 스냅샷·편집기·세션·탭·파일·확장) → 결과 반환.
3. 턴 끝 **후처리 훅**: ```sql 블록을 앱 포매터(사용자 포맷 설정)로 · 빈 줄 정리 → 대본 표시(코드 상자 [복사][편집기][▶][▦]).
4. 사용자 조작 = `/status`(연계 방식·에이전트 포함) · `/skills` · `/agent` · 설정 `ai.hook_format_sql` · `ai.hook_tidy`.

### 16-3 스킬 영역(9) · 에이전트(4) · 새 도구(6)
- 스킬: connections · metadata(검색 하네스) · editor · results · app · **tabs** · **objects** · **files** · **extensions**.
- 에이전트: assistant(전부) · schema(메타 읽기 전용) · reviewer(검토·explain·포맷) · dba(세션·락 사전 질의). 이력에는 저장하지 않는다(다시 열면 assistant).
- 도구: `list_tabs` · `switch_tab` · `list_files`(작업 루트 제한 걷기) · `open_file` · `list_extensions` · `set_extension_enabled`(승인).
- 이번 묶음의 수정: Insert into editor(분할 창 포커스 때 paste 무시 → 편집기 포커스 복귀 뒤 삽입) · MCP 서버 이름 `nexa` → `nexa-sql` · 시스템 프롬프트 스타일 규칙(간결).
- 같은 묶음의 작은 것: AI 툴바 그룹에서 [대화 탭] 버튼 제거(사용자 "chat tab 툴바는 제외" · `ai.side`만 · 채팅 탭은 활동 막대·팔레트) · **활동 막대 툴팁**(사용자 "좌측 아이콘에 툴팁이 없다" · hover 아이콘 오른쪽 캡슐 · 글 = 팔레트 명령 라벨 · 최상위 층) · 내장 모델 표에 `claude-fable-5-2`(사용자 "Fable 5.2가 목록에 안 보임" → API 프로필은 [저장 + 검증]이 `/v1/models`로 갱신 · CLI는 내장 표) · `/skills`.
- API 모드와 MCP(사용자 질문): API 모드는 요청 `tools`로 같은 도구를 직접 쓴다(앱이 호출자) · Anthropic의 원격 MCP 커넥터는 인터넷에서 닿는 서버여야 해서 로컬 `nexa-sql` 서버(접속·비밀 보유)는 노출하지 않는다 → MCP는 같은 PC의 `claude` CLI용 전달 수단.

### 16-4 실기 피드백 10차(2026-11-12 새벽 · 세션 탭 = 편집기 탭 규칙 · 활동 막대 = 패널만 · 기본 제공자 · 설정 트리)
| # | 실기 원문 | 구현 |
|---|---|---|
| 1 | 기본 제공자를 Claude로 바꿔도 탭이 Codex로 열림 | `ai.provider` 변경 즉시 `ai_apply_provider_default`(패널 탭 전환 · 뷰 제공자 · 동기화) — 종전엔 `ai_init` 때 한 번만 읽었다. |
| 2 | 활동 막대 Claude/Codex 클릭이 탭을 열어 쓰던 편집창이 바뀜 → 패널만 · 탭은 [⊕ 새 대화]·세션 목록에서 · **1클릭 = 미리보기 · 더블클릭 = 고정 · 고정 탭이 있으면 그 탭으로**(편집기 탭과 같은 기준) · 세션마다 탭 | 뷰 탭 열쇠 = **`ai.chat:<세션 id>` / `ai.codex:<세션 id>`**(세션마다 탭) · `Editors::open_view_tab_preview(key, title, pinned)` = 이미 열려 있으면 그 탭(고정 요청 = 승격) · 미리보기면 살아 있는 미리보기 탭 재사용(파일 미리보기였어도 뷰로 바꿈) · 없으면 새 탭(미리보기 슬롯) · 패널 세션 행 1클릭 `OpenSession`(미리보기) · 더블클릭 `OpenSessionPinned`(`ui.dblclick_ms`) · [⊕ 새 대화]·팔레트·기동 명령 = 고정 · `ai_follow_view`가 활성 탭 열쇠로 제공자·세션을 맞춘다(채팅 슬롯 복원 로직 제거) · 세션 삭제 = 그 탭 닫기 · 제목 변경 = 탭 제목 갱신 · 활동 막대 = 패널만. |
| 3 | AI 설정이 설정 창 트리 어디에? | 트리에 없었다(검색으로만) → `CATEGORY_TREE`에 **그룹 "AI" ▸ "AI 어시스턴트"** 추가. |

- 설정 키 그대로(29) · i18n +1(`GrpAi`) · 시험 그대로.
- **대기(사용자 11-12 새벽 접수 · 다음 묶음)**: ① AI 표시 레벨(`ai.verbosity` basic/detail · 기본 basic = 도구 호출/결과 줄 숨김 · 팔레트 "AI: 상세 보기" 토글) ② 객체 찾기 창 미리보기 줄 복사(Ctrl+C · 우클릭) ③ 메타 검색이 **코멘트**("기준정보 - 설비(워크센터)관리")도 보도록(`ObjEntry.comment` · 컬럼 코멘트 · 한국어 낱말) + 스킬에 "사용자 원문 낱말로 먼저" ④ AI 턴의 `run_query`마다 **새 결과 탭**(`ai.result_tabs` 기본 on · `fresh_result_tab_for_run`) ⑤ PostTurn 훅을 턴의 모든 어시스턴트 메시지에(협업 관찰) ⑥ **진행 상태 표시**(요청 줄 아래 요청 시각 `년월일 시:분:초.밀리초` + 경과 · 상태 변화 시각 = 최초 요청 · 첫 응답 · 응답 단계 시작 · 최종 응답 · 완료 전엔 계속 갱신 · 진행 단계 1·2·3 중 현재 단계 깜빡임/글 갱신) ⑦ **요청 고정 머리글**(VS Code처럼 스크롤해도 그 요청에 종속된 내용 위에 사용자 요청을 고정 · 객체 탐색기의 서버 머리 고정과 같은 부품) ⑧ CLI 모드 "`nexa-sql` MCP 서버 연결 30초 초과" 오류 보완(원인 = `nsql mcp-bridge` ↔ 앱 TCP 연결 실패 경로 분석 · 사전 점검·재시도·안내) ⑨ 메타 검색은 **코멘트**(`TABLE - APPDB.DMS_I301090 - 기준정보 - 설비(워크센터)관리`)를 먼저 쓰도록(③과 같은 요구 · 사용자 재강조).

### 16-5 대기 ①~⑨ 구현(2026-11-12 새벽 · 사용자 "AI 미비·추가 요청 전체 진행")
| # | 요구 | 구현 |
|---|---|---|
| ① 표시 레벨 | 기본은 알아보기 쉬운 것만 · 상세는 지금 수준 | 설정 **`ai.verbosity`**(basic 기본 / detail) · 팔레트 토글 **"AI: 상세 보기"**(`ai.detail`) · basic = 도구 호출·성공한 도구 결과 줄을 대본에서 숨김(실패 결과·승인 카드·오류·안내는 보임 · 코드 블록 번호는 유지) · detail = 도구 줄에 시각 접두(`HH:MM:SS.mmm ⚙ …`). |
| ② 하단 결과 줄 복사 | 객체 찾기 창 미리보기 줄 1개 복사 | 미리보기 줄 포커스에서 **Ctrl+C** = 그 줄 · 미리보기 줄 **우클릭** = [줄 복사] 메뉴(`FoCopyLine`). **결과 그리드**도 같은 뜻일 수 있어 우클릭 메뉴에 **[행 복사]**(우클릭한 행 전체를 TSV로 · `copy_row` · 11-12 새벽 자율) 추가. |
| ③·⑨ 코멘트 메타 검색 | `TABLE - APPDB.DMS_I301090 - 기준정보 - 설비(워크센터)관리`를 찾아야 | `search_objects`가 **객체 코멘트**(`ObjEntry.comment` · 소문자)에 모든 낱말이 있으면 350점으로 적중(이름 점수와 큰 쪽) · 행에 `comment=` 열 · `in_columns`면 읽힌 **컬럼 코멘트**도(250) · `metadata` 스킬: "사용자 원문 낱말(한국어 포함)로 먼저 · 이름은 코드라 뜻은 코멘트에 · 영어 동의어는 그 다음". |
| ④ 결과 탭 | 한 턴의 여러 조회 = 탭 추가(설정 · 기본 여러 탭) | **`ai.result_tabs`**(기본 on): `run_query`·▦ 그리드 보기마다 `fresh_result_tab_for_run`(새 결과 탭 · 결과 없으면 거둠 · 편집기의 "새 탭으로 실행"과 같은 길) · off = 현재 탭 재사용. |
| ⑤ 훅 범위 | 턴의 모든 어시스턴트 메시지 | 마지막 사용자 메시지 뒤의 어시스턴트 메시지 전부에 PostTurn 훅(도구 앞 메시지의 ```sql도 포맷). |
| ⑥ 진행 상태 | 요청 시각(년월일 시분초.밀리초) + 경과 · 응답 시작 · 완료 · 진행 단계 깜빡임 | `ChatMsg.at`/`done_at`(epoch ms · 이력에 저장) · **요청 줄 오른쪽** "요청 2026-11-12 01:02:03.456 · 12.3 s"(완료 전엔 매 틱 갱신 · 완료 뒤 최종 경과) · **응답 줄 오른쪽** "응답 01:02:05.120 …" → 완료 뒤 "응답 … · 완료 … · 37.8 s" · 하단 상태 왼쪽 **①②③**(1 요청 보냄 · 2 응답 수신 중 · 3 완료 = 안 그림) 현재 단계 500 ms 깜빡임(`AiView::tick`) · 종전 "진행 중 · 시작 · N s · 받은 글자" 그대로. |
| ⑦ 요청 고정 머리글 | VS Code처럼 요청을 위에 고정 | 대본을 내려 사용자 요청이 화면 위로 넘어가면 **대본 맨 위 띠**에 "You ▸ 요청 첫 줄"을 고정(다음 요청이 화면에 들어오면 밀려남 · 객체 탐색기 서버 머리 고정과 같은 꼴). |
| ⑧ MCP 30초 초과 | "nexa-sql 도구 서버 연결 실패" 오류 보완 | 원인 후보 = Claude Code의 MCP 시작 제한 기본 30 s(`MCP_TIMEOUT`) · 브리지 `nsql` 미발견 · 앱 리스너 불통. **보완** = 기동 전 사전 점검(브리지 경로 `bridge_program`/`bridge_available` · 리스너 자기 접속 2 s · 실패 = 즉시 오류 안내) · 설정 **`ai.mcp_timeout_secs`**(기본 120)를 `MCP_TIMEOUT`/`MCP_TOOL_TIMEOUT`로 CLI에 · `init mcp=[nexa-sql:failed]`면 진단 Note(포트·브리지·원문) + CLI 프로세스 내려 다음 보내기에 재기동. |

- 설정 키 +3(`ai.verbosity` · `ai.result_tabs` · `ai.mcp_timeout_secs` → AI 키 32) · i18n +18 · 이력 jsonl에 `at`/`done_at`(없던 줄 = 0 → 복원 때 지금 시각 유지).

### 16-6 실기 피드백 11차(2026-11-12 새벽 · MCP 실패 원인 확정 · 진행 표시 수준 · 모든 팝업 창 첫 화면)
| # | 실기 원문 | 원인·구현 |
|---|---|---|
| 1 | CLI 모드 "nexa-sql 도구 서버가 CLI에 붙지 못했다(`mcp=[nexa-sql:failed]`)" — 사전 점검(브리지·리스너)을 넣어도 재현 | **원인 확정(실측)**: Claude Code 2.1.319는 `initialize` 전에 **`server/discover`** 요청을 보낸다. 앱 MCP 서버가 `-32601 method not found`를 돌리면 CLI가 ≈45초를 더 기다린다(흉내 서버 실측: 빈 결과 8.7초 ↔ 오류 53초) → 종전 기본 제한 30초 안에 못 붙어 failed. 브리지(`nsql mcp-bridge`)는 단독 왕복 정상. **수정** = `mcp::handle`이 `server/discover`에 빈 결과(`{}`)를 바로 돌린다(단위 시험) · `ai.mcp_timeout_secs`(120)는 보험으로 유지. 사전 점검·진단 Note는 그대로. |
| 2 | 1분 넘도록 무슨 일이 일어나는지 모름(CLI 모드) | CLI 모드는 턴 내내 도구 호출만 오가는데 basic 레벨이 도구 줄을 전부 숨겼다 → basic에서도 **도구 호출은 한 줄**(`HH:MM:SS.mmm ⚙ 이름` · 인자 없음) · 성공 결과만 숨김 · 대본 끝에 **진행 활동 줄** "● 진행: <마지막 도구> · 도구 N회 · N s"(500 ms 깜빡 · 매 틱 갱신 · 첫 응답 전 = "첫 응답 대기") · 하단 ①②③은 좁아도 그린다(임계 120→40px). |
| 3 | 설정 창처럼 변수·객체 찾기·로그 등 **모든 팝업 창**이 첫 화면이 까맣다 | 공통화: `winfocus::attrs_headless`가 **모든 창을 숨긴 채** 만들고(헤드리스와 같은 길), `Presenter`가 **첫 present 직전에 보이고 present 뒤 활성화**(`show_pending` · Windows는 present→보이기 순서면 첫 픽셀을 버린다 · 설정 창 11-10의 길을 공통으로). 설정 창의 자체 처리·주 창의 자체 처리는 그대로(이중 호출 무해). 창 20개 전부 해당(winhost 경유 8 + 직접 생성 12). |

- 사용자 확인(11-12 새벽): 요청 고정 머리글 ✓ · 진행 상태 표시 ✓(표시 수준은 위 2로 수정) · 조회마다 새 결과 탭 ✓ · 세션 탭 ✓ · 객체 찾기 창 ✓.
- 설정 키 그대로(32) · i18n +2(`AiActivityLine` · `AiWaitingFirst`) · 시험 +1(mcp discover).
- ★ **두 번째 원인(11-12 오전 · 86eeac21 · 사용자 세 번째 지적 · [journal 11-12 §11](journal/2026-11-12.md))**: `discover` 수정 뒤에도 `nexa-sql:failed` 재현 → `ai/mcp.rs` `serve`가 읽기 제한(500 ms)마다 받던 줄 조각을 지워, 브리지가 줄 본문과 줄바꿈을 따로 쓰면 요청이 사라졌다(실측 119.5초 뒤 failed). 수정 = 줄 누적 보존 + `nodelay` · 브리지 한 번에 쓰기 → 연결 1~3초. 함께 = `ai.prewarm`(CLI 미리 띄우기 · 첫 응답 8.3 → 4.4초) · 실기 E2E `ai/e2e.rs`(`#[ignore]` · 협업 판정 = Claude·Codex 2/0 · `mcp=[nexa-sql:connected]`) · 회귀 시험 `request_split_across_read_timeout_is_answered`.

### 16-7 헤드리스 모의 도구 호출 · 에이전트 이력(2026-11-12 새벽 · 자율 진행 · 협업 제안)
- **`NSQL_AI_MOCK_TOOL="name:{json};name2:{json}"`**(모의 호스트 `start_mock`): 지정한 도구들을 차례로 `ToolCall → ToolRequest → ToolResult`로 부르고, 각 결과 앞 200자를 " → name: …" 글로 되돌린다(basic 레벨은 도구 결과 줄을 숨기므로 덤프 본문으로 판정). 없으면 종전 `get_setting` 하나. 이로써 `meta_stats` · `search_objects`(코멘트·컬럼) · `list_tabs` · `connect`(scope=private 승인 카드) 같은 흐름을 헤드리스 V1로 판정할 수 있다.
- **에이전트 이력 저장**: 세션 jsonl 머리에 `agent` — 다시 열어도 `/agent`로 고른 역할이 유지된다(종전 = assistant로 되돌아감).
- **승인 카드 "⚠ WHERE 없음"**(전 도구 훅 · 데이터 보호 불변식 취지): `run_query`의 UPDATE/DELETE 문장에 WHERE 낱말이 없으면 승인 카드 첫 줄에 "⚠ WHERE 없는 UPDATE/DELETE — 모든 행에 적용됩니다"(CLI·API 경로 공통 · `sql_dml_without_where` 순수 함수 + 시험 · 차단은 아니고 경고).
- **세션별 작성 상자 초안**: 세션을 바꿀 때(탭 열쇠 따라가기 · 분할 창 탭별 · 세션 열기 · 새 대화) 떠나는 세션의 글을 `AiState.drafts`에 저장하고 새 세션의 초안을 꺼낸다(빈 글 = 지움) — 종전엔 뷰 부품이 하나라 글이 세션을 넘나들었다(§15-13 한계 해소).
- **승인 카드 본문 = SQL**(API 경로 · `run_query`): JSON 입력 대신 SQL 본문을 보여 준다(그 밖 도구는 JSON 그대로) · WHERE 경고는 그 위 첫 줄.
- **CLI 도구 서버 연결됨 안내**: `init mcp=[nexa-sql:connected]`면 상태줄 "도구 서버 nexa-sql 연결됨(포트 N)"(대본엔 안 올림).

### 16-8 ★ P0 데이터 보호 결함(2026-11-12 새벽 · 협업 V1 관찰 · 즉시 수정)
- **결함**: AI `run_query`의 WHERE 없는 `DELETE`가 **행 데이터 동의 카드 하나만 허용해도 실행**됐다(쓰기 확인 카드·⚠ WHERE 경고가 안 뜸). 격리 홈 SQLite 복사본에서 3행 → 0행 재현(협업).
- **원인**: `ai_approve`의 `ApprovalReply::Rows` 허용 가지가 `ai_exec_tool`을 **직접** 불러, `ai_tool_request`의 쓰기 확인 분기(`tool.write && (!auto || !readonly_q)`)를 건너뛰었다. 동의 카드가 쓰기 카드보다 먼저 오는 순서라 쓰기 문장일수록 늘 이 길을 탔다.
- **수정**: 동의 허용 뒤 `ai_tool_request(name, input, reply)`로 **다시 진입** — 읽기 전용이면 실행 · 쓰기면 확인 카드(SQL 본문 + ⚠ WHERE)가 한 번 더 뜬다(Auto여도 비조회면 확인). 61 §1-7 "자동 편집은 1행" 불변식과 별개로, AI 경로의 쓰기는 **반드시 사용자 확인**을 거친다는 규칙을 여기서 지킨다.
- 덤: 도구 오류 글 "has no stored password: …"가 가림 필터(`password:` 꼴)에 걸려 망가지던 것 → 문구를 괄호/세미콜론으로.
- V1 재현 절차(협업)를 회귀 시험으로 유지: `NSQL_AI_MOCK_TOOL='run_query:{"sql":"DELETE FROM equip_master"}'` → 동의 허용 → **쓰기 확인 카드**가 떠야 하고 거부하면 행 수 그대로.
- **회귀 판정(협업 · 2a7c4879)**: 무 WHERE DELETE = 동의 → ⚠ 카드(SQL 본문) → 거부 3행 그대로 / 허용 0행 · WHERE 1=0 = ⚠ 없음 · Auto 켬에도 카드 · 모의 회귀 없음 ✓.
- **이중 안전장치(`AiState.write_ok`)**: 승인 카드 [허용]이 (도구 · 입력)을 기록하고, `ai_exec_tool`의 `run_query`는 **비조회 SQL이면 그 기록이 같을 때만** 실행(한 번 쓰고 비움) — 어떤 경로가 확인을 건너뛰어도 실행 단계에서 막히고 "차단: … 승인 카드 없이 실행 단계" 오류 줄. 읽기 전용 SQL·get_result·explain은 **동의만으로 실행**(협업 관찰 "SELECT도 카드 두 번" 해소).

### 16-9 과정 쿼리 숨김 실행 · 최종 조회문 자동 표시(2026-11-12 오후 · 사용자 요청 · d0320796)
- **숨김 실행**: AI 도구 `run_query`·`explain`은 `PendingTool::silent`로 실행 — 결과를 그리드에 싣지 않고(`run_events` `on_ev_result_set`), 편집기 `source_sql`도 바꾸지 않는다. 결과는 도구 응답으로만 AI에게 간다.
- **도구 대기열** `ai.tool_queue`: 세션이 바쁠 때 온 조회 도구는 "session busy" 오류 대신 줄을 서서 차례로 실행 · 턴이 끝나면 비운다.
- **최종 조회 자동 표시** `ai.show_result`(auto 기본 / manual): 정상으로 끝난 턴의 마지막 답에 있는 **마지막 ```sql 블록**이 조회문이면 결과 그리드에 실행 · 쓰기 문장은 절대 자동 실행 안 함 · 채팅 뷰 탭이 활성이면 탭을 뺏지 않으려고 건너뜀(▦ 수동).
- **결과 탭 고르기** `ai_run_grid`: 활성 결과 탭이 비어 있으면(실행 이력 없음) 그 탭에 · 아니면 새 결과 탭.
- **기본 레벨 도구 줄** = `HH:MM:SS.mmm ⚙ run_query · SELECT …`(인자 요약 70자).
- **헤드리스 판정(협업 · [journal 11-12 §12](journal/2026-11-12.md))** = 숨김 ✓ · 연속 도구 대기열 ✓ · 자동 표시 → 빈 결과1 ✓ / 이력 있으면 결과2 ✓ · 도구 한 줄 ✓ · F5 회귀 없음 ✓. 관찰 = 숨김 실행이어도 편집기(또는 채팅 뷰) 오른쪽 아래 **실행 카드**("select 1 · 완료 1행")와 상태줄 행 수가 남고 **Output 탭**이 새로 생김(그리드·결과 탭은 그대로) · 채팅 뷰에서는 실행 카드가 작성 상자 오른쪽 위에 겹쳐 뜸 → 개발 판단.
- 모의 판정 요령: 모의 호스트는 도구 결과 뒤 글(" → name: …")이 "마지막 답"이 되므로, 자동 표시를 보려면 도구 결과 안에 ```sql 블록이 오게 한다(예: SQLite `select char(10)||'```sql'||char(10)||'select …'||char(10)||'```' as x`) · 동의 카드 대기 중 편집기 탭으로 바꾼 뒤 허용.

### 16-10 Output·Log 도구 · 자동 표시 검증 · 빈 세션 재사용(2026-11-12 오후 · 94d0ea5b · fdce771c)
- **도구** `get_output`(Output 탭 꼬리) · `get_log`(실행 로그 꼬리 · `errors_only`) — 오류 문의는 로그 먼저(스킬 안내).
- **자동 표시 검증**: §16-9의 최종 조회문이 실행 오류면 포맷한 답을 원문으로 되돌리고 원문을 다시 실행 · 포맷과 무관한 오류는 대본에 표시.
- **빈 세션 재사용**(`ai_new_session`): 같은 제공자에 대화 없는 세션이 있으면 새로 만들지 않고 그것으로 간다 — 분할 창이 탭마다 빈 "(new)"를 만들던 것과 "지우면 곧바로 새 빈 세션이 생겨 삭제가 안 되는 것처럼 보이던 것"을 함께 해소.
- **헤드리스 판정(협업 · [journal 11-12 §14](journal/2026-11-12.md))** = 도구 2종 ✓ · 탭 3개 분할 창 [+] = 세션 1 ✓ · 패널 [+] 두 번 = 2 ✓ · 현재 세션 삭제 1 → 0 ✓.

### 16-11 편집기 연동 보강 · 도구 27(2026-11-12 저녁 · 8c084629)

- **도구 27** = list_commands · run_command · get_setting · set_setting · get_editor_text · set_editor_text · **edit_editor** · format_sql · search_objects · list_tabs · switch_tab · list_files · open_file · list_extensions · set_extension_enabled · meta_stats · describe_object · object_source · list_profiles · session_info · connect · run_query · get_result · get_output · get_log · explain · skill.
- ★ **11-14 추가(0094d7c8)** = `bookmarks`(읽기 · list|goto · filter · scope · id · mnemonic) · `edit_bookmark`(쓰기 · add|rename|note|mnemonic|delete · 편집기 탭 필요) · `search_files`(읽기 · 비동기 · 검색 패널을 열고 채움 · query · case · word · regex · where(패널 문법 "폴더, *.sql") · where_case · limit) · `reveal_object`(읽기 · 비동기 · 객체 탐색기로 전환해 표/컬럼 선택 · name · schema · member) · `disconnect`(쓰기 · scope=current|all · 미커밋 연결은 남김) · `open_file` line·column · 새 스킬 `bookmarks` · objects(reveal_object) · files(search_files) · connections(disconnect) 스킬 갱신 · 협업 판정 = [journal 11-14 §5](journal/2026-11-14.md)(✗ search_files case·regex 토글 미적용).
- `get_editor_text` = 본문 + `<caret line column total_lines/>` · `<selection>` · `<statement_at_caret lines="a-b">`(AI가 "커서가 있는 문장"을 정확히 집도록).
- `edit_editor`(쓰기 · 승인 카드) = op `replace`(+ `text`) · `sort_asc` · `sort_desc` · `unique` · `reverse` · `upper` · `lower` · `trim` · `select` · 범위 = `from_line`/`to_line`(없으면 선택 → 커서 문장) · 범위 밖 = 오류 · 되돌리기 = Ctrl+Z 한 단계 · 채팅 탭이 활성이면 거부(편집기 탭 또는 분할 창에서).
- `explain` = 운영에서도 실행(문장 단위 · 2단 무장·보류 커밋 건너뜀 = 계획만 보고 데이터는 안 바꿈 · analyze 고정 끔).
- 판정 = [journal 11-12 §19](journal/2026-11-12.md)(헤드리스 전부 ✓ · Oracle 실서버 explain·끌기 놓기 = 사용자 실기).

## 17. 데이터 보호 하네스(2026-11-12 오후 · 사용자 "요청과 실행 사이의 제어 · 운영은 보호 우선 · 대량 삭제/수정/테이블 제거는 최소 2단계" · e3b675bc)

> 원문 = `crates/nexa-sql/src/ai/guard.rs` 머리 주석(순수 계층 · 시험 = 정책 표 전수 56 + 불변식 4). 이 절은 요약·판정 기록.

### 17-1. 계층

```text
AI 도구 호출(run_query · set_setting · run_command …)
  │ ① 분류  classify_tool / classify_sql → Risk
  │         Read < AppWrite < Write < Bulk < Destructive < TxEnd < Protection(여러 문장 = 가장 높은 것)
  │ ② 문맥  Ctx { prod(ConnEnv::Prod) · auto(세션 Auto) · auto_write(ai.guard_auto_write=dml) }
  │ ③ 판정  decide(Risk, Ctx) → Allow | Card | TwoStage | Deny
  ▼
게이트 1 = 승인 카드(운영 ⛔ 줄 · 등급 ▲ 줄 · WHERE 없음 ⚠ · SQL 본문)
  ▼ (TwoStage만)
게이트 2 = 영향 확인 카드 "2단계 중 2단계 — 영향 확인 · 영향 행 수: N"(impact_count_sql = 실행 전 SELECT COUNT(*))
          운영 = run.prod_confirm_ms(기본 3000) 지나야 [허용]이 먹음
  ▼
실행 = 사람 경로와 같은 run_text_in(보류 커밋 · DDL 가드 · 백업 그대로) · 게이트 한 곳 = ai_tool_request
  ▼
커밋 = 운영은 사람만(보류 커밋 안내 "N행이 커밋되지 않았습니다")
```

### 17-2. 정책 표(`decide`)

| 등급 | 예 | 비운영 | 운영 |
|---|---|---|---|
| Read | SELECT · explain | 바로 실행 | 바로 실행 |
| AppWrite | 편집기 글 · 일반 설정 · 접속 · 확장 · 변수 삭제 `delete_var`(11-13 · 952e4728) | Auto면 바로 · 아니면 카드 | 카드 |
| Write | INSERT · WHERE 있는 UPDATE/DELETE · CREATE · COMMENT | `ai.guard_auto_write=dml`이면 바로 · 아니면 카드 | 카드(자동 승인 안 닿음) |
| Bulk | WHERE 없는 UPDATE/DELETE · MERGE · 블록/EXEC · 스크립트 명령 · 영향 행 ≥ `ai.guard_bulk_rows`(1000) | 2단계 | 2단계 |
| Destructive | DROP · TRUNCATE · ALTER · RENAME · CREATE OR REPLACE · GRANT/REVOKE | 2단계 | 2단계 |
| TxEnd | COMMIT · ROLLBACK | 카드 | **거부** |
| (AppWrite 예외 · 11-14 8b82af2a) | `run_command conn.disconnect`(접속 해제) | 카드 | **카드**(실행 메뉴 명령이지만 거부 아님) · 세션 실행 중/보조 작업 중 · 미커밋(tx_pending·prod_held)이면 "not executed: … busy / uncommitted changes …" |
| (Read로 내림 · 11-14 0094d7c8) | `run_command view.*`(패널·창 열기) · `edit_editor op=select`(줄 선택) | 바로 실행 | 바로 실행 |
| (AppWrite · 11-14 0094d7c8) | `edit_bookmark` · `disconnect` 도구 · `run_command conn.disconnect` = 공유 연결 선택 메뉴 없이 끊음(카드에 "이 연결을 쓰는 탭 N개") | 카드 | 카드 |
| (Read로 내림 · 11-12 c4b91fb9) | 변수 대입 = `set_var`(값·수식) · `run_query`가 `EXEC :이름 := 식`/클라이언트 명령만 | 바로 실행 | 바로 실행(보류 건너뜀) |
| Protection | 보호 설정(`run.prod_*` · `tx.*` · `backup.*` · `ai.guard_*` · `ai.auto` · `ai.codex_auto` · `ai.permission_mode` · `ai.enabled` · `ai.max_rows`) · 실행 메뉴 명령(`run.*` · `tx.*` · `hold.*` · `conn.*` · `sess.*` · `grid.apply*` · `import.*`) | 카드(Auto여도) | **거부** |

### 17-2-a. 승인 카드 머리 줄(11-12 저녁 · 873160ce)

- 게이트 1 카드와 행 데이터 동의 카드 맨 위 3줄 = "목적: …"(`run_query` 새 필수 인자 `purpose` · 없으면 "(어시스턴트가 이유를 적지 않았습니다)") · "무엇을: <종류> · 대상: <객체>" · "어디서: <프로필> (운영/비운영)" → 그 아래 등급·경고 줄과 SQL 본문.
- 협업 판정 = 동의 카드에 세 줄 + SQL 본문 ✓ · purpose 없을 때 안내 ✓([journal 11-12 §18](journal/2026-11-12.md)).

### 17-2-b. 쓰기 카드 한 장 · 운영 두 번 눌러 허용(11-13 저녁 · 81ba5e5b)

- 쓰기 `run_query`(카드 대상)는 행 데이터 동의 카드를 따로 띄우지 않는다 — 쓰기 확인 카드 한 장에 동의 줄("허용하면 이 세션 동안 이 데이터베이스의 행 데이터도 어시스턴트에게 보냅니다")을 싣고, 허용 = 동의 + 실행. 조회·자동 승인된 쓰기는 예전대로 동의 카드.
- 운영 접속 + 쓰기 + 2단계 대상 아님 = [허용] 첫 누름은 장전("한 번 더 눌러 실행 (N초)" + 남은 시간 막대) · `run.prod_confirm_ms`(기본 3000) 안 두 번째 = 실행 · 지나면 원래 버튼 · 장전 뒤 350 ms 안 연타는 버림 · 거부는 즉시. 2단계 대상(WHERE 없는 DML · 파괴적)은 첫 카드 한 번 누름 → 영향 확인 카드(기존 대기) 그대로.
- 협업 판정 = 헤드리스 6/6 ✓([journal 11-13 §8](journal/2026-11-13.md)).
- ★ **영향 확인(2단계)도 같은 카드(11-13 밤 · 732d8bc4)** = 새 카드 없이 방금 허용한 카드에 "2단계 중 2단계 — 영향 확인 · 영향 행 수: N" 줄만 덧붙이고 [허용]이 장전(빨강 + 왼쪽부터 차는 게이지 + "허용 (남은 초)")으로 돌아온다 · 옛 "N초 뒤 승인" 대기 삭제 · 2단계도 제한 시간 안 두 번 눌러 허용(지나면 처음부터) · 2단계 대상의 첫 누름엔 "허용함" 줄 없음 · 자동 승인 쓰기(`ai.guard_auto_write`) + `ai.guard_bulk_rows`에 걸린 것 = 카드가 없던 길이라 영향 줄 + SQL 새 카드 한 장 · 협업 판정 ✓([journal 11-13 §10](journal/2026-11-13.md)).

### 17-3. 운영 불변식 4(시험으로 고정)

1. 운영에서 데이터를 바꾸는 요청은 절대 바로 실행되지 않는다.
2. 대량·파괴적 변경은 어디서나 2단계.
3. 운영의 트랜잭션 끝·보호 변경은 거부(사람만).
4. 보호 변경·트랜잭션 끝은 어떤 자동 승인으로도 풀리지 않는다.

### 17-4. 설정 2

- `ai.guard_auto_write` = off(기본) / dml — 비운영의 일반 변경(Write) 자동 승인. 운영에는 닿지 않는다.
- `ai.guard_bulk_rows` = 1000(기본) — 영향 행 수가 이 이상이면 Write를 Bulk로 올려 2단계.

### 17-5. 닫은 구멍(e3b675bc · 조사 보고 1·2·3·4·6·7·9)

- 보호 장치에 막힌 실행(읽기 전용 프로필 · 백업 대기 등)을 **성공으로 보고**하던 것 → 실패("not executed: …")로.
- Auto에서 `set_setting`·`run_command`가 카드 없이 실행되던 것 → 보호 키·실행 명령은 Protection(Auto여도 카드 · 운영 거부).
- 조회 판정 = 방언 분류(`TxClass`) + 문자열 판정 둘 다(SELECT … INTO · FOR UPDATE는 조회 아님).
- CLI 권한 프롬프트와 도구 요청의 이중 카드 → 게이트 한 곳(`ai_tool_request`).
- 숨김 실행 잔여 = 과정 쿼리는 실행 카드·Output 탭을 만들지 않음.
- (조사 보고 번호와 각 항목의 대응은 개발 세션 보고 기준 · 이 목록은 커밋 메시지 요약)

### 17-6. 남은 것

- `run.explain_analyze` 사람 경로(EXPLAIN ANALYZE는 실제 실행 · AI `explain`은 analyze 고정 끔).
- ~~객체 백업(objbk) 재진입 결과 처리~~ → ✅ 11-13 b74ae98c(백업으로 미뤄진 `run_query`는 pending 유지 · `ai.objbk_wait` 동안 시간 초과 판정 멈춤 · 백업 뒤 재진입 실행 결과를 같은 도구 호출로 · 운영 백업 실패 = 실패로 답함 · ■ = 미뤄 둔 실행 취소 · 협업 V1 ✓ [journal 11-13 §11](journal/2026-11-13.md) · ■의 결과 글 = bc228a9b에서 "cancelled … NOT executed" / "stopped … may already have been sent"로 · 백업 대기 중 다른 연결 탭 전환 = 실행 안 함(`OutObjBackupTabMoved`) · [journal 11-13 §12](journal/2026-11-13.md)).
- 전체(whole) 실행 범위 불일치.

### 17-7. 헤드리스 판정(협업 · [journal 11-12 §15](journal/2026-11-12.md))

전부 ✓ — (a) WHERE 없는 DELETE = 동의 → ▲ 대량 카드 → 영향 3행 카드 → 허용 0행 / 거부 3행 그대로 · (b) WHERE id=1 UPDATE = 카드 1장 → 실행 · `ai.guard_bulk_rows=1`이면 영향 1행 카드 · (c) `auto_write=dml` = (b) 카드 없이 · (a)는 여전히 2단계 · (d) DROP TABLE = ▲ 파괴적 → 영향 3행 · (e) 운영 = commit · `run.prod_confirm` off · `run.all` 셋 다 거부 줄 + 도구 오류 · `auto_write=dml`에도 ⛔ 카드 · 게이트 2를 1초 만에 허용 = 무시 → 4초 뒤 허용 = 실행 + 보류 커밋 안내 · (f) Auto + `tx.prod_hold` = 카드 / `editor.word_wrap` = 바로 · (g) 숨김 실행 = 실행 카드·Output 탭 없음 · F5는 종전대로 · (h) 읽기 전용 프로필 UPDATE = "not executed: 읽기 전용 프로필 …"(ok=false).
관찰 = 게이트 2를 열린 지 약 1초 만에 [허용]하면 무시되고 카드가 남는 것(✓)은 확인했지만, 상태줄 덤프에는 "N초 뒤" 안내가 잡히지 않음(다른 자리에 보이는지 실기 확인) · 숨김 실행 직후 상태줄 오른쪽 칸에 그 조회의 "3행 · 0.000s"가 남음(실행 카드·Output 탭은 안 생김).

## 18. AI ↔ 편집기 연동(2026-11-12 밤 · 사용자 요청 · 5ba8e377)

### 18-1. 동작 6

| # | 동작 | 진입점 |
|---|---|---|
| 1 | 편집기 → AI 요청(설명 · 오류 수정 · 최적화 · 첨부 · 마지막 오류) | 편집기 우클릭 "AI ▸" · 팔레트 `ai.explain_selection` · `ai.fix_statement` · `ai.optimize_statement` · `ai.attach_selection` · `ai.fix_error` · 사용자 메시지 = 프롬프트 + "(편집기 a~b행)" + ```sql 본문 |
| 2 | 실행 오류 → AI | 실행 오류 카드의 복사 버튼 왼쪽 [AI](`ai.error_offer` · **기본 off** · 첫 AI 턴 뒤 한 번 [켜기] 추천 토스트 `notice.ai_error_offer`) · Output 우클릭 · 별도 오류 토스트는 없앰(b6ff48ba) |
| 3 | AI 답 → 편집기 | 코드 블록 [교체](`ai.replace_target` = 선택/문장) · 답의 줄 번호 밑줄 클릭 = 그 줄로(`ai.line_links`) |
| 4 | 그리드 → AI | 결과 그리드 "고급 복사 ▸ AI에 첨부"(첨부 칩) |
| 5 | 편집 승인 | `edit_editor`/`set_editor_text` 승인 카드 = 변경 미리보기(`@@` · `-` · `+` · 문맥 `ai.diff_context`) |
| 6 | 답의 표 | 마크다운 표 열 맞춤 + 머리 밑줄 |

### 18-2. 변수 도구(도구 30)

- `list_vars` · `get_var`(이름 · 형 · 층 · 종류 · 현재 값) · `set_var`(쓰기 · 카드 1 · 세션 변수는 클라이언트에 산다 = DR-8).
- `delete_var`(11-13 · 952e4728 · 사용자 "AI가 변수를 삭제할 수 있게 · 모든 AI · API/CLI 공통") = AppWrite · 카드 1 · 도구 표 `tools::TOOLS` 하나라 API·CLI(MCP `nexa-sql`)·Codex(`dynamicTools`) 공통 · 변수 창 줄 삭제와 같은 길(`vars_apply(Delete)`) · 답 = `deleted <이름> (layers: …)` + 그 이름을 참조하던 수식 변수 안내 · 프로필 층만 있는 변수 = `not deleted`(읽기 전용) · 없는 이름 = 오류 · 협업 V1 ✓([journal 11-13 §6](journal/2026-11-13.md)).
- 승인 카드 처리 중 표시(11-13 · 952e4728 · 사용자 "누른 뒤 끝날 때까지 Disable + 진행 표시 · 다른 버튼에도 쓸 기법") = 누르면 두 버튼 비활성 + 누른 버튼 아래 진행 막대(60 ms 다시 그리기) · 도구 실행·세션 작업이 끝나고 최소 시간(허용 400 ms · 거부 250 ms)이 지나면 버튼 줄을 거둠(`AiState.approving` · `ai_tick_due` · `paint_busy_button`) · 처리 중 재클릭 = 무시.
- `run_query`가 `EXEC :V := 리터럴`만이면 앱 쓰기 등급(카드 1 · 행 데이터 동의 없음) — 협업 V1에서 SQLite 세션은 3장(동의 · 대량 2단계 · 영향 알 수 없음)으로 나옴 = 결함 후보(§18-4).

### 18-3. 설정 8

| 키 | 기본 | 뜻 |
|---|---|---|
| `ai.send_target` | side | 편집기 요청을 보낼 곳(side = 분할 창 · tab = 채팅 탭) |
| `ai.error_offer` | **off**(b6ff48ba) | 실행 오류 카드에 [AI] 버튼 |
| `ai.meta_first` | on(2872aa63 · 11-14) | 카탈로그 조회 전에 앱이 적재한 메타(search_objects 등)를 먼저 — 아니면 턴당 1회 되돌림 |
| `ai.meta_auto` | on(2872aa63 · 11-14) | 카탈로그 조회는 행 데이터 동의 카드 없이 |
| `ai.replace_target` | selection | 코드 블록 [교체] 대상(selection = 선택 · statement = 커서 문장) |
| `ai.diff_context` | 3 | 변경 미리보기의 앞뒤 문맥 줄 수 |
| `ai.line_links` | on | 답의 "N행"·"a~b행"을 밑줄 링크로 |
| `ai.prompt_explain` · `ai.prompt_fix` · `ai.prompt_optimize` | (내장 문구) | 편집기 요청 프롬프트 문구 |

### 18-3-a. 후속(11-12 밤 · b6ff48ba)

- 편집기 요청(설명·수정·최적화·첨부)은 지금 세션을 그 탭의 분할 창 세션으로 묶어 보이는 자리에 띄움(`ai_bring_pane`).
- `EXEC :V := 리터럴`만의 run_query = `local_assign`(클라이언트 처리 · 카드 1 · 행 동의 없음).
- 모든 쓰기 도구 승인 카드 = "목적:"(purpose 자동 인자) · "무엇을:"(예: "변수 v1의 값을 zz(으)로 설정(앱 안에서만 · DB로 보내지 않음)") · "어디서:".
- 변경 미리보기 = 편집기 전체 기준 `@@ <시작 줄> @@` + 앞뒤 문맥 `ai.diff_context`(3)줄.
- 편집기 우클릭 메뉴(하위 포함) 클릭은 라우팅 맨 앞에서 편집기로.

### 18-3-b. 변수 수식 · 토글 명령(11-12 밤 · c3ff1460)

- `set_var`에 `formula` 인자 = `EXEC :이름 := 수식`을 숨김 실행하고 답으로 get_var 상태를 돌려줌 · 결과 집합 없는 도구 실행의 답 = "completed…".
- `run_command`의 토글 명령(`view.*` 등) = 상태 먼저 확인(`state` = on 기본 · off · toggle) · 이미 열린 창은 닫지 않고 앞으로("already on/open - left as it is (brought to the front)").
- 협업 판정 = [journal 11-12 §22](journal/2026-11-12.md)(수식 값 5 ✓ · 토글 3단계 ✓ · 관찰 = SQLite 세션의 `set_var` formula 답이 `kind: value`(formula 아님 · `formula`·`depends on` 줄 없음 · 값 5는 맞음) · 이름이 `V3`(대문자)로 나옴).

### 18-4. 판정(협업 · [journal 11-12 §20](journal/2026-11-12.md))

- ✓ 변수 도구 3 · 변경 미리보기 카드 · 오류 알림 버튼 + `ai.fix_error` 메시지 · 편집기 요청 메시지("(편집기 1~3행)" + sql) · 줄 링크 밑줄 · 표 열 맞춤 · 그리드 AI에 첨부.
- 결함 후보 2(→ b6ff48ba에서 수정 · [journal 11-12 §21](journal/2026-11-12.md) ✓) = ★ 결함 후보 ① = 편집기 명령 `ai.explain_selection`(그리고 `ai.fix_error`)을 실행하면 요청은 새 세션에서 돌지만, 분할 창에는 그 세션이 아니라 빈 "(new)" 세션이 뜸(명령 직후 0.2초부터 · 패널에서 요청 세션을 열어야 대화가 보임) · ★ 결함 후보 ② = SQLite 세션에서 `run_query` `EXEC :v1 := 'b'`가 앱 쓰기 카드 1장이 아니라 행 데이터 동의 → "▲ 대량 변경 — 2단계 검증 대상" → "영향 행 수: 알 수 없음(셀 수 없는 문장)" 3장(스크립트 명령 = Bulk로 분류된 듯 · 방언 의존 여부 확인 필요).
- 관찰 = 변경 미리보기 머리 줄이 2행 교체인데 `@@ 1 @@`이고 `ai.diff_context`(기본 3)인데 앞뒤 문맥 줄이 안 보임 · 그리드 첨부 칩 글 "grid (1 rows)"(영어 복수형).
