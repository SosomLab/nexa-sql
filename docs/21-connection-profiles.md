# 21 · 연결 프로필 — 사용자 폴더에 암호화 저장 · CLI/GUI · 여러 인스턴스 공유

> 사용자 요구(09-13): *"사용자 폴더에 접속 정보를 암호화해서 저장해두고 연결 시 재사용"* · *"몇 개의 Instance를 실행하든 저장된 암호를 함께 사용"*.
> 결정 = [10 DR-22](10-decision-record.md). 구현 = `crates/nsql-vault` · CLI `nsql conn` · GUI 이름 칸 + Save. D-2(OS 키체인 여부)는 이 문서로 닫힘.

## 1. 저장 위치·형식

```text
<사용자 설정 폴더>/nexa-sql/              nexa_conf::user_config_dir("nexa-sql")
│                                        Windows %APPDATA%\nexa-sql · macOS ~/Library/Application Support/nexa-sql · Linux ~/.config/nexa-sql
│                                        환경변수 NSQL_HOME이 있으면 그 폴더(테스트 · 개발용 재지정)
├─ device.key                            기기 키 32B — Windows: "NSDK"‖ver‖DPAPI 블롭 · 그 외: 평문 0600
└─ profiles/<이름>.conf                  프로필 1개 = 파일 1개(nexa-conf key=value · 원자적 쓰기)
     _schema=1
     dialect=oracle  user=scott  host=db  port=1521  database=orcl  role=SYSDBA
     secret=<hex>                        비밀번호 봉투만 암호화 — 나머지는 평문(목록에 보여야 하는 값)
```

- **프로필 이름** = `[A-Za-z0-9_.-]{1,64}`(선행 `.` 금지). 접속 문자열에 반드시 들어가는 `:`·`@`·`/`가 없으므로 **`-c prod`처럼 접속 문자열 자리에 이름을 그대로** 쓸 수 있다. 이름 꼴인데 프로필이 없으면 오류(오타를 접속 문자열로 오해하지 않게).
- **비밀번호 봉투** = ChaCha20-Poly1305 · 키 = SHA-256("nexa-sql/dr-22/sealed-v1" ‖ `profile-v1/<이름>` ‖ salt ‖ 기기 키) · AAD에 도메인 포함. `dev.conf`의 `secret=`을 `prod.conf`에 옮겨 붙여도 열리지 않는다(도메인 분리). 형식·태그가 어긋나면 **fail-closed**(평문인 척 통과 없음). 봉투 코드는 nexa-clip `nclip-store/src/sealed.rs` 이식.
- **기기 키 보호**: Windows = DPAPI(`CryptProtectData` · 사용자 범위 · 앱 엔트로피 · UI 금지 플래그) → 같은 Windows 계정만 푼다. macOS·Linux = 평문 0600(정직한 한계 — 폴더째 복사엔 못 버팀 · Keychain/Secret Service 결합은 후속 D-18). 평문 32B 파일은 어느 OS에서나 읽으므로 unix에서 만든 폴더를 Windows로 옮길 수도 있다(반대는 불가).

## 2. 여러 인스턴스 공유 (사용자 요구)

| 상황 | 동작 |
|---|---|
| GUI 창 여러 개 · CLI 병렬 · 둘 혼용 | 같은 폴더·같은 `device.key`를 읽으므로 **어느 프로세스든 같은 비밀번호를 푼다**. 메모리 캐시 없음 — 다른 인스턴스가 방금 저장한 프로필도 다음 접속에 바로 보인다 |
| **동시 첫 실행**(기기 키가 아직 없음) | `create_new`로 한 프로세스만 생성 성공 · 진 쪽은 이긴 쪽 파일을 읽는다(빈 파일이면 10ms 간격 재시도). 8 스레드 동시 생성 테스트로 단일 키 보장 |
| 같은 프로필 동시 저장 | 파일 단위 원자적 rename — 마지막 저장이 이기고 손상은 없다 |
| 다른 OS 계정 · 다른 PC | Windows DPAPI 키는 열리지 않음(의도) — 그 계정에서 `nsql conn add`로 다시 저장 |

## 3. 사용법

```text
nsql conn list                                  NAME · DIALECT · PW(✓/-) · user@host:port/db
nsql conn add <name> <target> [-d dialect] [-p pw] [--no-prompt]
                                                target에 비밀번호가 없고 방언이 sqlite가 아니면 숨김 입력(Windows 콘솔 모드 · unix stty)
nsql conn show <name> · rm <name> · test <name>(실접속) · path(폴더)
nsql run|shell|export -c <name> …               이름 → 프로필
스크립트  CONNECT <name>                          사용자명만 있는 CONNECT는 프로필 해석기를 거친다(nsql-run Resolver)
GUI       접속 칸에 이름만 넣고 Connect · [이름 칸] + [접속 문자열] + Save → 저장(비밀번호는 봉투)
```

## 4. 계층·경계

- `nsql-vault`(Core 옆 · UI 무의존): `Vault::open_default/save/get/peek/list/remove/resolve` · `is_profile_name`. 의존 = `nsql-core` · `nsql-script`(ConnectSpec) · `nexa-conf`(폴더·직렬화·원자적 쓰기) · 암호화 3종(원장).
- `nsql-run::Runner.resolver`(옵션): 호스트가 주입 — `CONNECT <이름>` 해석. 없으면 지금처럼 그대로 접속.
- 호스트(CLI·GUI 워커)는 `-c`/접속 칸 값을 먼저 `is_profile_name`으로 보고 저장소에서 푼 뒤 `parse_target`으로 간다.

## 5. 열린 것
- **D-18** macOS Keychain · Linux Secret Service로 기기 키 보호(현재 0600 평문). 결합 시 `device.key`만 교체 — 프로필 재암호화 불요(키 간접 한 겹의 이유).
- GUI 프로필 **목록 선택**(콤보/로그인 리스트 — Golden식 · [22](22-driver-extensions.md) §1) · 프로필 삭제 UI — 지금은 이름 입력 + Save만.
- 프로필별 드라이버 버전 고정(`driver=<id>@<ver>`) — [22 §4](22-driver-extensions.md).

## 5. Demo 프로필 · 샘플 데이터(사용자 09-17)

- **무엇**: 서버 없이 바로 써 볼 수 있는 로컬 SQLite — 프로필 이름 **`Demo`** · 파일 `<사용자 설정 폴더>/demo.sqlite`(`NSQL_HOME` 규약 · exe 옆 금지 · 포터블 D-78과도 같은 트리) · 내용 = `examples/demo.sql`(SCOTT `dept`/`emp` + `sales` 5,000행) — 스크립트는 **바이너리에 내장**(`include_str!` · DR-27 배포본에 파일 없음).
- **언제**: ① **최초 실행 1회** 팝업 "로컬 SQLite 데모를 만들까요?"(지금 만들기 / 나중에) — 설정 `demo.prompted`(HIDDEN · 자동 기억 · 끄면 다음 시작 때 다시) ② **도움말 ▸ 샘플 데이터 만들기(Demo 프로필)…** — 프로필과 파일이 **둘 다 있으면 비활성**(nexa-ctl 풀다운 `MenuEntry::Disabled`).
- **어떻게**: 배경 스레드 `nsql-demo`가 `nsql_drivers::open(sqlite:…)` 세션에 러너 `run_script`로 내장 스크립트를 실행 → 오류가 하나라도 있으면 파일을 지우고 실패 안내(토스트) → 성공하면 `Vault::save("Demo", spec)` · 접속 창 목록 갱신 · 메뉴 비활성 · 상태줄/로그 "Demo 프로필 준비됨 — 'Demo'로 접속해 SELECT * FROM emp;". 파일이 이미 있으면(프로필만 없던 경우) 스크립트는 건너뛰고 프로필만 만든다.
- **CLI**: `nsql run -c Demo examples/demo.sql`처럼 같은 프로필을 쓴다 · `nsql conn add Demo sqlite:<경로>`로 손수 만들 수도 있다. 테스트 `demo_db_is_created_from_embedded_script`.

## 6. GUI 실행 인자로 바로 접속(사용자 09-17)

```
nexa-sql <프로필>                    # 저장된 프로필로 시작하면서 접속(폼도 그 프로필로 채움)
nexa-sql -c oracle://u:pw@h:1521/svc  # 접속 문자열로 접속(-c/--connect · 맨 인자도 됨)
nexa-sql sqlite:demo.sqlite
nexa-sql --fill <프로필>             # 폼만 채우고 접속하지 않음(종전 동작)
nexa-sql --help                      # 사용법
```

프로필·문자열 모두 **Connect 버튼과 같은 경로**(`ConnectSpec` → `last_spec`)를 타므로 접속 뒤 오브젝트 탐색기도 붙는다(종전 T-104 결함 = 문자열 접속이 `last_spec`을 안 채워 탐색기 "Not connected"). 프로필이 없으면 상태줄 안내. 스펙으로 못 푸는 문자열은 종전처럼 워커 `Connect(문자열)`.
