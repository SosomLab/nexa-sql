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
