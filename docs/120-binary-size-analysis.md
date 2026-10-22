# 120 · Release 실행 파일 크기 — 내용 분석과 축소 방안 (134차 win · 2026-10-21 · 검토)

> 요구(사용자 10-21): "현재 Release 실행 파일 크기가 너무 큰데 내용 분석과 줄일 수 있는 방법 검토" + 질문 "심볼/변수/문자열 길이를 익명화하면 .text를 획기적으로 줄일 수 있나?"
> 상태 = ⏸ **보류(사용자 10-22 "용량 절감 처리는 보류 업무로 등록 · 분석 정보는 잘 보관") · 측정·결정안은 보관** = TODO **T-382** · 10 §3-1 D-330~D-336a·D-343 보류 등재 · 종전 상태 = 검토(측정 = 협업 세션 · 10-21) · 10-22 cargo-bloat 측정 = §5-5 · tiberius 0.13 사전 검토 = §5-6 · 원자료 = 부록 §A. 기준 = v0.2.7 뒤 작업 트리(10-21 12:35 Release) · GUI `nexa-sql.exe` **21.69 MB** · CLI `nsql.exe` **11.27 MB**.

## 0. 한 줄 결론

식별자 익명화는 **효과 0에 가깝다**(§1). 크기의 뿌리는 ① **최적화 수준** — 전역 `opt-level = 3` 코드가 가장 크다(측정 B = 전역 `"s"`만으로 **21.7 → 16.1 MB · −26 %** · 비핵심 의존만 `s`(A) = −7 %) ② 드라이버 묶음(Oracle Thin + aws-lc-rs · SQL Server tiberius + **rustls 0.21 + ring** = **TLS 스택 둘**) ③ 제네릭 단형화가 많은 크레이트(tokio · serde · regex/fancy-regex · wasmi · 드라이버) ④ UI 코드(.text ≈ 8.2 MB = GUI − CLI 차이) ⑤ 내장 DBMS 아이콘 PNG 692장 **1.6 MB**(처음 적은 3.3 MB는 `du` 블록 합 = 과대). 현실적 목표 = **GUI 21.7 → 약 13~15 MB(−30~40 %)** = 전역 `"s"` + 핫 패스만 `3`(변형 F · 성능 실측 조건) + 아이콘 외부화(−1.6 MB) + TLS 스택 단일화(aws-lc 통일 · −1~1.5 MB) + 기능 플래그 정리. 측정으로 확정(§4·§5).

## 1. 질문에 대한 답 — 심볼·변수·문자열 익명화

| 대상 | 어디에 있나 | 지금 상태 | 익명화 효과 |
|---|---|---|---|
| 함수·변수 **이름**(심볼) | 심볼 표·디버그 정보(.text 아님) | `strip = "symbols"`로 **이미 제거** · 기계어는 이름 길이와 무관 | **0** |
| 지역 변수 이름 | 컴파일 뒤 존재하지 않음(레지스터·스택 슬롯) | — | **0** |
| 문자열 리터럴(i18n 메시지 · 설정 키·설명 · 로그 · 오류 글 · `expect("…")` 메시지 · 패닉 위치 `파일:줄`) | **.rdata**(읽기 전용 데이터 · .text 아님) | GUI .rdata 4.81 MB 중 PNG 3.3 MB를 빼면 **문자열·표 ≈ 1.5 MB** | 줄여도 수백 KB(전체의 1~3 %) · 패닉 위치의 **경로 문자열**은 `--remap-path-prefix`로 짧게(수십~수백 KB) |
| 타입 이름(`core::any::type_name` · 패닉 메시지의 타입 글) | .rdata | 소량 | 미미 |

→ .text(16.5 MB)는 **기계어**다. 줄이는 길은 코드 양(단형화 · 기능 플래그 · 크레이트 제외)과 최적화 수준(`opt-level`)뿐이다.

## 2. 사실(측정 · 10-21)

| 항목 | GUI `nexa-sql.exe` | CLI `nsql.exe` | 비고 |
|---|---|---|---|
| 전체 | 21,692,416 B | 11,270,656 B | 10-21 12:35 · 12:32 |
| .text(기계어) | **16,506,656**(76 %) | 8,313,104(74 %) | GUI − CLI ≈ **8.2 MB = UI 층**(nexa-ctl · nexa-gfx · nexa-font · winit · softbuffer · 앱) |
| .rdata(상수·문자열·내장 파일) | **4,812,552**(22 %) | 2,688,362 | GUI 쪽 +2.1 MB ≈ 아이콘 PNG |
| .pdata/.reloc/.rsrc/.data | ≈ 0.37 MB | ≈ 0.22 MB | — |
| 프로필 | `opt-level = 3` · `lto = "fat"` · `codegen-units = 1` · `panic = "abort"` · `strip = "symbols"` | 같음 | **이미 공격적** → 남은 손잡이 = `opt-level = "s"/"z"`(전역 또는 패키지별) |
| 내장 자산 | DBMS 아이콘 PNG **692장 1.6 MB**(파일 바이트 합 · `crates/nexa-sql/assets/dbms` · `build.rs` `include_bytes!` 전부 · 처음의 3.3 MB는 `du` 4 KB 블록 합) · 문법 `.sqlg` 6 = 28 KB · `demo.sql` | 아이콘 없음 | 173종 × 크기 여러 벌 = 692 파일 |
| 의존 크레이트 | **230**(GUI) | — | 09-22 인벤토리 165 → +65(Thin · 아이콘 · wasmi · 123~133차) |
| TLS 스택 | **둘** = rustls 0.21 + `ring`(tiberius ← tokio-rustls 0.24) · rustls 0.23 + `aws-lc-rs`/`aws-lc-sys`(oracledb Thin) | 둘 | 암호 프리미티브 두 벌 = 추정 1.5~2.5 MB 중복 · v0.2.0 때 Thin+rustls로 +4 MB 실측(125차) |
| 무거운 크레이트 후보 | tokio 1.53 · tokio-postgres · tiberius · oracledb · rusqlite/libsqlite3-sys(SQLite 전체 내장 ≈ 1.5 MB) · wasmi 1.1 + wasmparser(WASM 실행기 ≈ 1~2 MB) · regex + regex-syntax(유니코드 표) · fancy-regex · encoding_rs(CJK 표 ≈ 0.3~0.5 MB) · chrono · winit 0.30 | — | 크레이트별 실측 = §4 측정 1 |

## 3. 축소 손잡이(추정 · 측정으로 확정)

| # | 손잡이 | 추정 절감(GUI) | 위험·비용 | 판정(초안) |
|---|---|---|---|---|
| S-1 | **DBMS 아이콘 외부화** — 설치본(DR-27)이므로 `share/dbms-icons.pack`(PNG 묶음 1파일 · 색인) 또는 설치 폴더의 PNG를 **지연 적재**(지금도 지연 디코드) · exe엔 generic 틀 + 대표 5종만 | **−1.6 MB**(−7 %) | 파일 하나 더 배포(MSI/pkg/deb/rpm 패키징 수정 33) · 없으면 generic 아이콘으로 폴백 · 포터블 없음(DR-27)이라 경로 안정 | ✅ 1순위 |
| S-2 | **TLS 스택 단일화** — 협업 조사(§5-4): `ring` 통일은 **불가**(oracledb · tiberius 0.13 모두 aws-lc 강제 · 포크 필요) → 가능한 길 = **tiberius 0.13으로 올려 aws-lc로 통일**(rustls 0.21 + ring 제거 · 주의 = 0.13 기본 `command_timeout` 30 s → `None`으로 두어야 장시간 질의가 끊기지 않음) | **−1~1.5 MB**(ring + rustls 0.21) | tiberius 0.12 → 0.13 API 차이 · TLS 동작 재검증(SQL Server encrypt) · 동일성·grid-edit E2E | ✅ 2순위(aws-lc 통일) |
| S-3 | **전역 `opt-level = "s"` + 핫 패스만 `3`**(변형 F = nexa-gfx · nexa-ctl · nexa-font · nsql-core · nsql-run) — 측정 A(비핵심 의존만 `s`) = **−1.5 MB(−7 %)**뿐 · B(전역 `s`) = **−5.6 MB(−26 %)** → 크기의 몸통은 **이 저장소 자체 크레이트**의 3 코드 | **−4~5 MB**(F 추정 · B와 A 사이) | 핫 패스 벤치(nexa-ui 70만 줄 88.9 ms · 기동 · 10만 행 페치)로 회귀 0 확인 조건 | ✅ **1순위로 승격**(측정 F 뒤 확정) |
| S-4 | 전역 `opt-level = "s"`(B) 또는 `"z"`(C) | B = **−5.6 MB(실측)** · C = 측정 중 | 앱 크레이트까지 `s` → 편집기·그리드·래스터 핫 패스 회귀 가능(벤치 필수) | B = F가 못 미치면 후보 · C = 실측만 |
| S-5 | **regex 유니코드 표·기능 플래그** — `regex` 기본 피처(unicode 전체) → 필요한 것만(`unicode-perl` · `unicode-case` · 한글 = `unicode-script`? 검토) · fancy-regex 동반 | −0.3~0.8 MB | 찾기/필터의 `\w`·대소문자 무시·한글 낱말 경계 동작 → 시험 자산(find · filterq)으로 회귀 확인 | ✅ 검토 |
| S-6 | encoding_rs 표 | −0.3 MB | 파일 인코딩 CJK 지원 필수 → **제외 불가**(필요 표만 남기는 피처 없음) | ✗ |
| S-7 | wasmi(WASM 확장 실행기) | −1~2 MB | 확장 시스템(75) 핵심 → 제외 불가 · 피처(`std` 외) 정리만 | 피처 정리만 |
| S-8 | tokio 피처 최소화(`rt` · `net` · `io-util` · `time`만 · `full` 금지) | −0.2~0.6 MB | 드라이버 crate가 요구하는 피처 확인 | ✅ 검토 |
| S-9 | 경로 문자열 `--remap-path-prefix`(패닉 위치 · `file!()`) | −0.05~0.2 MB | 없음(로그 가독성만) | ✅ 쉬움 |
| S-13 | **`wasmi` 기본 기능 `wat` 끄기**(협업 측정 1 발견) — WAT 텍스트 파서 `wast` 500 KB가 `wasmi → wat → wast`로 끌려옴 · 저장소·확장 적재 코드에 WAT 사용 0(바이너리 `.wasm`만) → `wasmi = { default-features = false, features = ["std"] }` | **−0.5~0.6 MB** | 확장이 `.wat`를 직접 적재하지 않는다는 규칙 명시(75) · 시험 = 확장 적재 E2E | ✅ 쉬움 · 즉시 |
| S-10 | 실행 파일 압축(UPX류) | −50 % 겉보기 | **✗** — 백신 오탐 · 기동 때 전부 풀어 메모리 +전체 · 92 보호 검토와 배치 | ✗ |
| S-11 | 드라이버를 확장 cdylib로 분리(DR-29 재검토) | GUI −4~6 MB · 설치본 합계 동일 | DR-29(내장 정적 유지) 뒤집기 · 지연 dlopen 설계 · 배포 파일 수 | ⏸ 보류(설치본 총량은 안 줄어든다 · 사용자 결정) |
| S-12 | `panic = "unwind"`(D-329 요구) | **+**(증가 · 언와인드 표 · 추정 +2~5 %) | 119 §5-4 비고 = 실측 뒤 판단 | 측정 2에 포함 |

## 4. 측정 계획(협업 · 별도 `target-size/` · 사용자 인스턴스 영향 0)

1. **크레이트별 기여** — 심볼 유지 Release 사본(`CARGO_PROFILE_RELEASE_STRIP=false` · `CARGO_TARGET_DIR=target-size`) → `llvm-nm --size-sort --demangle`(llvm-tools 있음) → 심볼 접두(`크레이트::`)로 합산 → 상위 40 크레이트 · 상위 50 심볼(단형화 폭발 후보) · `.rdata` 큰 항목(아이콘 묶음 · 유니코드 표).
2. **프로필 변형**(각 전체 Release 1회 · 순차 · 백그라운드): A = 패키지별 `opt-level="s"`(드라이버·tokio·regex·wasmi·serde·winit·oracledb·tiberius·tokio-postgres·rusqlite) · B = 전역 `s` · C = 전역 `z` · D = **`panic = "unwind"`**(D-329) · E = A + S-9. 각 = 전체/.text/.rdata + 기동 시간(26 §7 B) + nexa-ui 벤치 1회(핫 패스 회귀 감시).
3. **아이콘 외부화 산술** — `build.rs` 내장 합계(3.3 MB)와 외부 pack 설계안(S-1) · 패키징 영향 목록(33).
4. **TLS 단일화 가능성** — tiberius 최신판의 rustls 판 · oracledb의 `ring` 피처 유무(상류 Cargo.toml 확인 · 네트워크) → S-2 실행 경로 2개 중 선택지와 절감 추정.
5. 결과 = 이 문서 §5 표 채움(협업) → 개발이 §6 결정안.

## 5. 측정 결과(협업 채움)

측정 조건 = Windows 11 · `CARGO_TARGET_DIR=target-size`(사용자 Debug·`target/release` 영향 0) · 기준 소스 = 작업 트리(10-21 오후 · 커밋 전 134차 변경 포함) · cargo 한 번에 하나 · 협업 하위 에이전트 2(아이콘 · TLS)는 cargo 없이 병렬.

### 5-1. 크레이트별 기여(측정 1)

방법 = 기준 프로필 그대로 + 링커 맵(`RUSTFLAGS=-Clink-arg=-MAP` · Git Bash가 `/MAP`을 경로로 바꿔 1차 실패) · MSVC exe에는 심볼 표가 없어(PDB) llvm-nm 대신 **맵의 심볼 주소 차**로 크기를 셈 · 크레이트 = Rust v0 이름의 첫 크레이트 루트(`Cs…_<len><name>`) / C 라이브러리는 Lib:Object 열 · ⚠ 제네릭 단형화는 **제네릭을 정의한 크레이트**(core·alloc·hashbrown)로 잡힌다 · 무명 상수·문자열(아이콘 PNG 포함)은 크레이트를 모른다 · 기준 빌드 = 14:43~14:53(9m41s) · GUI 21,442,560 B(12:35 `target/release` 21,692,416과 −0.25 MB 차 = 그 사이 소스 변경 · 빌드 폴더 차이).

**묶음 합계(GUI · 상위 80 크레이트)**

| 묶음 | 바이트(상위 40 크레이트 안) | % |
|---|---|---|
| 앱 본체(nexa_sql) | 4,160,504 | 19.7 |
| 무명 상수·문자열(.rdata · 아이콘 PNG 포함) | 3,700,178 | 17.5 |
| 표준 라이브러리(core·alloc·std·hashbrown) = 단형화 포함 | 2,666,106 | 12.6 |
| TLS·암호(rustls·aws_lc·ring·webpki·sct·tokio_rustls·rustls_pki_types) | 2,353,830 | 11.1 |
| SQLite(C + rusqlite) | 1,510,765 | 7.1 |
| nsql 코어 층(nsql_*) | 1,462,808 | 6.9 |
| WASM 확장(wasmi·wasmparser·wast·wat·wasm-encoder·leb128) | 1,275,960 | 6.0 |
| UI(nexa_ctl·nexa_gfx·nexa_font·nexa_dlg·nexa_sys·winit·softbuffer) | 1,091,589 | 5.2 |
| regex 계열(regex·regex_automata·regex_syntax·aho_corasick·fancy_regex·memchr) | 778,648 | 3.7 |
| Oracle(oracledb Thin + odpic OCI + 드라이버) | 647,094 | 3.1 |
| SQL Server(tiberius + 드라이버) | 314,689 | 1.5 |
| PG(tokio_postgres·postgres·postgres_protocol + 드라이버) | 252,704 | 1.2 |
| encoding_rs | 203,335 | 1.0 |
| tokio·futures | 44,092 | 0.2 |
| 그 밖(상위 40 안 나머지) | 567,837 | 2.7 |
| 맵 합계 | 21,169,179 | 100 |

**눈에 띄는 것**
- ★ **`wast` 500,360 B(2.4 %)** = WAT 텍스트 파서 — `wasmi` 기본 기능 `wat`(Cargo.lock: wasmi → wat → wast)가 끌어오는데 저장소 코드는 WAT를 쓰지 않음(`wat::`·`parse_str` 0건) → `wasmi = { version = "1.1", default-features = false, features = ["std"] }`로 **≈ −0.5~0.6 MB**(wast + wat + wasm-encoder · 새 손잡이 S-13 후보 · 확장 `.wat` 직접 적재를 안 쓰는지 개발 확인).
- TLS·암호 2.35 MB(11.1 %) = aws_lc_sys C 1,304,856 + rustls(0.21+0.23 합) 519,560 + ring C 355,516 + ring Rust 67,414 + webpki 66,048 + tokio_rustls 14,848 … → 경로 1(tiberius 0.13)로 빠지는 쪽 = ring 422 KB + rustls 0.21 몫 + webpki 0.101 + sct + tokio_rustls 0.24 ≈ **0.6~0.8 MB**(§5-4 추정 0.5~1.0과 맞음).
- SQLite C 1.50 MB(7.1 %) · Oracle 0.65 MB(oracledb Thin 275 KB + ODPI-C 162 KB + 드라이버) · SQL Server 0.31 MB · PG 0.25 MB = **드라이버 하나당 0.25~0.65 MB + 그 드라이버의 TLS/C 라이브러리**(MySQL·ODBC 내장 D-342 예상치의 기준).
- 앱 본체 `nexa_sql` 4.16 MB(19.7 %) · 표준 라이브러리 2.67 MB(대부분 단형화 = 앱·드라이버가 만든 제네릭) · regex 계열 0.78 MB · UI 층 1.09 MB · encoding_rs 0.20 MB.
- .rdata 무명 상수·문자열 3.70 MB 중 아이콘 PNG 1.60 MB(§5-3) · 나머지 ≈ 2.1 MB = 문자열·표(i18n `Msg` 표 · 유니코드 표 · 문법 `.sqlg` · 패닉 위치 문자열 …).

#### GUI nexa-sql.exe(기준 · 21,442,560 B · .text 16,331,907 · .rdata 4,750,160)

맵 합계 = .text 계열 16,331,907 B · .rdata 계열 4,837,272 B(심볼 주소 차 · 정렬 패딩 포함 · 무명 상수는 앞 심볼에 붙음)

| # | 크레이트 | .text B | .rdata B | 합계 B | % |
|---|---|---|---|---|---|
| 1 | `nexa_sql` | 4,157,200 | 3,304 | 4,160,504 | 19.7 |
| 2 | `(무명 상수/문자열 · LTO 객체)` | 0 | 3,311,363 | 3,311,363 | 15.6 |
| 3 | `core` | 1,556,621 | 20,080 | 1,576,701 | 7.4 |
| 4 | `(C/외부) libsqlite3_sys` | 1,327,853 | 173,200 | 1,501,053 | 7.1 |
| 5 | `(C/외부) aws_lc_sys` | 765,536 | 539,320 | 1,304,856 | 6.2 |
| 6 | `nexa_ctl` | 610,224 | 576 | 610,800 | 2.9 |
| 7 | `std` | 544,569 | 544 | 545,113 | 2.6 |
| 8 | `rustls` | 519,328 | 232 | 519,560 | 2.5 |
| 9 | `wast` | 500,352 | 8 | 500,360 | 2.4 |
| 10 | `regex_automata` | 412,320 | 40 | 412,360 | 1.9 |
| 11 | `(기타 · LTO 객체)` | 37,019 | 351,796 | 388,815 | 1.8 |
| 12 | `wasmparser` | 368,624 | 8 | 368,632 | 1.7 |
| 13 | `(C/외부) ring` | 163,024 | 192,492 | 355,516 | 1.7 |
| 14 | `alloc` | 351,834 | 0 | 351,834 | 1.7 |
| 15 | `nsql_script` | 339,648 | 120 | 339,768 | 1.6 |
| 16 | `nsql_catalog` | 336,992 | 56 | 337,048 | 1.6 |
| 17 | `wasmi` | 331,088 | 8 | 331,096 | 1.6 |
| 18 | `oracledb` | 275,008 | 192 | 275,200 | 1.3 |
| 19 | `encoding_rs` | 72,368 | 130,967 | 203,335 | 1.0 |
| 20 | `nsql_run` | 197,792 | 0 | 197,792 | 0.9 |
| 21 | `hashbrown` | 192,458 | 0 | 192,458 | 0.9 |
| 22 | `tiberius` | 183,872 | 4,464 | 188,336 | 0.9 |
| 23 | `nexa_dlg` | 185,008 | 8 | 185,016 | 0.9 |
| 24 | `(C/외부) odpic_sys` | 132,510 | 29,072 | 161,582 | 0.8 |
| 25 | `regex_syntax` | 146,800 | 0 | 146,800 | 0.7 |
| 26 | `winit` | 137,534 | 6,064 | 143,598 | 0.7 |
| 27 | `nsql_i18n` | 139,472 | 48 | 139,520 | 0.7 |
| 28 | `aho_corasick` | 135,296 | 0 | 135,296 | 0.6 |
| 29 | `nsql_format` | 129,136 | 0 | 129,136 | 0.6 |
| 30 | `nsql_driver_mssql` | 126,336 | 17 | 126,353 | 0.6 |
| 31 | `tokio_postgres` | 121,312 | 8 | 121,320 | 0.6 |
| 32 | `nexa_gfx` | 97,120 | 135 | 97,255 | 0.5 |
| 33 | `ttf_parser` | 83,040 | 0 | 83,040 | 0.4 |
| 34 | `nsql_driver_oracle` | 78,992 | 80 | 79,072 | 0.4 |
| 35 | `aes` | 74,373 | 8 | 74,381 | 0.4 |
| 36 | `nsql_driver_pg` | 70,832 | 8 | 70,840 | 0.3 |
| 37 | `nsql_core` | 70,144 | 0 | 70,144 | 0.3 |
| 38 | `wasmi_ir` | 68,400 | 0 | 68,400 | 0.3 |
| 39 | `oracle` | 67,488 | 32 | 67,520 | 0.3 |
| 40 | `ring` | 65,232 | 2,182 | 67,414 | 0.3 |

상위 50 심볼(v0 이름 앞 110자)

| # | B | 세그 | 크레이트 | 심볼 |
|---|---|---|---|---|
| 1 | 223,164 | 2 | `(기타 · LTO 객체)` | `switch.table._RNvXs1g_NtCs7uUmbfRqPp3_4core3fmtRNtNtCsljG5A88lni3_5winit6window11WindowLevelNtB6_5Debug3fmtBA_` |
| 2 | 192,928 | 1 | `nexa_sql` | `_RNvXNtNtCs2veEDnD9Ew3_8nexa_sql3app10event_loopNtB6_3AppINtNtCsljG5A88lni3_5winit11application18ApplicationHa` |
| 3 | 156,816 | 1 | `nexa_sql` | `_RNvMNtNtCs2veEDnD9Ew3_8nexa_sql3app5paintNtB6_3App5paint` |
| 4 | 151,552 | 2 | `(C/외부) ring` | `ecp_nistz256_precomputed` |
| 5 | 151,552 | 2 | `(C/외부) aws_lc_sys` | `ecp_nistz256_precomputed` |
| 6 | 133,920 | 1 | `nsql_i18n` | `_RNvMs_CsbvHnsLs8xA3_9nsql_i18nNtB4_3Msg3row` |
| 7 | 117,328 | 1 | `nexa_sql` | `_RNvCs2veEDnD9Ew3_8nexa_sql4main` |
| 8 | 103,648 | 1 | `nexa_sql` | `_RNvMNtNtCs2veEDnD9Ew3_8nexa_sql3app6eventsNtB6_3App12drain_events` |
| 9 | 94,336 | 1 | `regex_automata` | `_RNvNtNtCskRPhzLqQ5cM_14regex_automata4meta8strategy3new` |
| 10 | 82,032 | 1 | `nexa_sql` | `_RNvNtCs2veEDnD9Ew3_8nexa_sql8explorer11meta_thread` |
| 11 | 78,496 | 1 | `nexa_sql` | `_RNvMs3_NtCs2veEDnD9Ew3_8nexa_sql8explorerNtB5_8Explorer5drain` |
| 12 | 70,512 | 2 | `(무명 상수/문자열 · LTO 객체)` | `anon.960cd60f6a10ad50a508225949193f91.3243` |
| 13 | 65,664 | 2 | `(C/외부) aws_lc_sys` | `p521_g_pre_comp` |
| 14 | 56,368 | 1 | `wasmi_ir` | `_RNvXs3_NtCsh56Qn6X4B28_8wasmi_ir4enumNtB5_2OpNtNtCs7uUmbfRqPp3_4core3fmt5Debug3fmt` |
| 15 | 51,192 | 2 | `(무명 상수/문자열 · LTO 객체)` | `anon.39bb33db066b7d310a03861422076907.1119` |
| 16 | 50,928 | 1 | `nexa_ctl` | `_RNvMs6_NtNtCs4JndnCx8TUR_8nexa_ctl8controls7textboxNtB5_7TextBox15paint_multiline` |
| 17 | 50,672 | 1 | `nexa_sql` | `_RNvMs_NtNtCs2veEDnD9Ew3_8nexa_sql3app10event_loopNtB8_3App16aux_window_event` |
| 18 | 48,160 | 1 | `nexa_sql` | `_RNvMNtNtCs2veEDnD9Ew3_8nexa_sql3app6eventsNtB6_3App10drain_conn` |
| 19 | 47,616 | 1 | `nexa_dlg` | `_RNvMs0_Cs57o7KKms4ES_8nexa_dlgNtB5_10FilePicker9open_menu` |
| 20 | 47,568 | 1 | `nexa_sql` | `_RNvMs_NtCs2veEDnD9Ew3_8nexa_sql5intelNtB4_5Intel7request` |
| 21 | 47,104 | 1 | `wast` | `_RNCNvXsI_NtNtCs6RbwrJmvTgm_4wast4core4exprNtB7_11InstructionNtNtBb_6parser5Parse5parse0Bb_` |
| 22 | 44,416 | 1 | `nexa_sql` | `_RNvMNtNtCs2veEDnD9Ew3_8nexa_sql3app11startup_cmdNtB6_3App11startup_cmd` |
| 23 | 40,720 | 1 | `nexa_dlg` | `_RNvMs0_Cs57o7KKms4ES_8nexa_dlgNtB5_10FilePicker3new` |
| 24 | 40,000 | 2 | `(C/외부) aws_lc_sys` | `kObjects` |
| 25 | 38,240 | 1 | `core` | `_RINvNtCs7uUmbfRqPp3_4core3ptr9drop_glueNtCs2veEDnD9Ew3_8nexa_sql3AppEBD_` |
| 26 | 38,048 | 1 | `nexa_sql` | `_RNvMs1_NtCs2veEDnD9Ew3_8nexa_sql4gridNtB5_4Grid11paint_inner` |
| 27 | 37,680 | 2 | `encoding_rs` | `_RNvNtCsbPVWBjdrBjD_11encoding_rs4data13BIG5_LOW_BITS` |
| 28 | 37,584 | 1 | `pkcs5` | `_RINvNtNtCs4D76ZfHUr6b_5pkcs55pbes210encryption16decrypt_in_placeINtNtCsg1aDjJ6Ud3l_5alloc3vec3VechEECs3lB9qod` |
| 29 | 36,496 | 1 | `tiberius` | `_RNCNvMs_NtNtCsbcUCBi2yBC_8tiberius6client10connectionINtB6_10ConnectionINtNtCslnKtCOGRuQA_10tokio_util6compat` |
| 30 | 35,816 | 1 | `(C/외부) libsqlite3_sys` | `sqlite3VdbeExec` |
| 31 | 35,792 | 1 | `nexa_sql` | `_RNvMs_NtCs2veEDnD9Ew3_8nexa_sql8conn_winNtB4_7ConnWin5paint` |
| 32 | 35,680 | 1 | `nexa_sql` | `_RNCNCNvNtCs2veEDnD9Ew3_8nexa_sql6worker5spawn0s5_0B7_` |
| 33 | 34,016 | 1 | `tiberius` | `_RNCNCNvMNtNtNtCsbcUCBi2yBC_8tiberius3tds6stream5tokenINtB6_11TokenStreamINtNtCslnKtCOGRuQA_10tokio_util6compa` |
| 34 | 33,984 | 1 | `wasmi` | `_RNvNtNtNtCs82cO2ScqvEZ_5wasmi6engine8executor6instrs14execute_instrs` |
| 35 | 33,216 | 1 | `encoding_rs` | `_RNvMs_NtCsbPVWBjdrBjD_11encoding_rs7variantNtB4_14VariantEncoder20encode_from_utf8_raw` |
| 36 | 32,752 | 1 | `nsql_catalog` | `_RNvNtCsbPUrUGZ7BVJ_12nsql_catalog3gen10object_ddl` |
| 37 | 32,608 | 1 | `nexa_sql` | `_RNvMNtNtCs2veEDnD9Ew3_8nexa_sql3app10extensionsNtB6_3App16apply_extensions` |
| 38 | 31,376 | 1 | `wast` | `_RNvNtNtCs6RbwrJmvTgm_4wast4core6binary6encode` |
| 39 | 30,792 | 2 | `(무명 상수/문자열 · LTO 객체)` | `anon.5f46119b08f84688b0b321c06fed6b04.19` |
| 40 | 30,720 | 2 | `(C/외부) aws_lc_sys` | `p384_g_pre_comp` |
| 41 | 30,720 | 2 | `(C/외부) aws_lc_sys` | `k25519Precomp` |
| 42 | 30,432 | 1 | `oracledb` | `_RNvMNtCs3lB9qodjTfL_8oracledb6clientNtB2_6Client7connect` |
| 43 | 30,336 | 1 | `nexa_sql` | `_RNvMNtNtCs2veEDnD9Ew3_8nexa_sql3app4metaNtB6_3App16explorer_actions` |
| 44 | 29,952 | 1 | `nexa_sql` | `_RNvMNtNtCs2veEDnD9Ew3_8nexa_sql3app5menusNtB6_3App16build_menus_with` |
| 45 | 29,280 | 1 | `oracledb` | `_RNvMs0_NtCs3lB9qodjTfL_8oracledb9transportNtB5_24CustomClientCertResolver8populate` |
| 46 | 29,072 | 1 | `nsql_drivers` | `_RNvCs2MBTVFLBHrM_12nsql_drivers4open` |
| 47 | 28,864 | 1 | `nexa_sql` | `_RNvMNtNtCs2veEDnD9Ew3_8nexa_sql3app7findobjNtB6_3App20findobj_window_event` |
| 48 | 28,192 | 1 | `nsql_driver_pg` | `_RNvMs5_CsfikaJPJTaIV_14nsql_driver_pgNtB5_9PgSession13execute_inner` |
| 49 | 26,336 | 1 | `nsql_catalog` | `_RNvCsbPUrUGZ7BVJ_12nsql_catalog11source_with` |
| 50 | 26,336 | 1 | `encoding_rs` | `_RNvMNtCsbPVWBjdrBjD_11encoding_rs7variantNtB2_14VariantDecoder18decode_to_utf8_raw` |

#### CLI nsql.exe(기준 · 11,042,304 B · .text 8,151,363 · .rdata 2,634,360)

맵 합계 = .text 계열 8,151,363 B · .rdata 계열 2,717,856 B(심볼 주소 차 · 정렬 패딩 포함 · 무명 상수는 앞 심볼에 붙음)

| # | 크레이트 | .text B | .rdata B | 합계 B | % |
|---|---|---|---|---|---|
| 1 | `(C/외부) libsqlite3_sys` | 1,327,853 | 173,248 | 1,501,101 | 13.8 |
| 2 | `(무명 상수/문자열 · LTO 객체)` | 0 | 1,354,611 | 1,354,611 | 12.5 |
| 3 | `(C/외부) aws_lc_sys` | 765,536 | 538,504 | 1,304,040 | 12.0 |
| 4 | `core` | 701,131 | 20,080 | 721,211 | 6.6 |
| 5 | `rustls` | 520,528 | 232 | 520,760 | 4.8 |
| 6 | `regex_automata` | 406,720 | 40 | 406,760 | 3.7 |
| 7 | `(C/외부) ring` | 163,024 | 192,492 | 355,516 | 3.3 |
| 8 | `nsql_catalog` | 314,848 | 56 | 314,904 | 2.9 |
| 9 | `oracledb` | 274,784 | 192 | 274,976 | 2.5 |
| 10 | `nsql` | 263,072 | 176 | 263,248 | 2.4 |
| 11 | `(기타 · LTO 객체)` | 34,860 | 219,612 | 254,472 | 2.3 |
| 12 | `alloc` | 228,584 | 0 | 228,584 | 2.1 |
| 13 | `std` | 224,092 | 528 | 224,620 | 2.1 |
| 14 | `encoding_rs` | 70,432 | 125,703 | 196,135 | 1.8 |
| 15 | `nsql_script` | 192,800 | 7 | 192,807 | 1.8 |
| 16 | `tiberius` | 184,128 | 4,464 | 188,592 | 1.7 |
| 17 | `(C/외부) odpic_sys` | 132,512 | 29,072 | 161,584 | 1.5 |
| 18 | `regex_syntax` | 147,184 | 0 | 147,184 | 1.4 |
| 19 | `nsql_i18n` | 139,456 | 41 | 139,497 | 1.3 |
| 20 | `nsql_run` | 139,312 | 0 | 139,312 | 1.3 |
| 21 | `aho_corasick` | 135,296 | 0 | 135,296 | 1.2 |
| 22 | `nsql_format` | 128,624 | 0 | 128,624 | 1.2 |
| 23 | `nsql_driver_mssql` | 126,928 | 0 | 126,928 | 1.2 |
| 24 | `tokio_postgres` | 121,456 | 8 | 121,464 | 1.1 |
| 25 | `nsql_driver_oracle` | 86,544 | 72 | 86,616 | 0.8 |
| 26 | `aes` | 74,373 | 7 | 74,380 | 0.7 |
| 27 | `nsql_driver_pg` | 71,200 | 0 | 71,200 | 0.7 |
| 28 | `ring` | 65,344 | 2,182 | 67,526 | 0.6 |
| 29 | `oracle` | 66,896 | 32 | 66,928 | 0.6 |
| 30 | `webpki` | 65,552 | 576 | 66,128 | 0.6 |
| 31 | `nsql_driver_oracle_thin` | 64,816 | 32 | 64,848 | 0.6 |
| 32 | `sha2` | 62,240 | 8 | 62,248 | 0.6 |
| 33 | `nsql_io` | 52,662 | 0 | 52,662 | 0.5 |
| 34 | `chrono` | 51,376 | 1,024 | 52,400 | 0.5 |
| 35 | `pkcs5` | 52,032 | 0 | 52,032 | 0.5 |
| 36 | `nsql_settings` | 48,992 | 96 | 49,088 | 0.5 |
| 37 | `hashbrown` | 42,003 | 0 | 42,003 | 0.4 |
| 38 | `nsql_core` | 40,960 | 0 | 40,960 | 0.4 |
| 39 | `tokio` | 36,960 | 112 | 37,072 | 0.3 |
| 40 | `(C/외부) nsql.res` | 0 | 35,208 | 35,208 | 0.3 |


### 5-2. 프로필 변형(측정 2 · 진행 중)

| 변형 | 내용 | GUI 전체 B | GUI .text | GUI .rdata | CLI 전체 B | CLI .text | 빌드 시간 |
|---|---|---|---|---|---|---|---|
| 기준(12:35 `target/release`) | `opt-level=3` · fat LTO · cgu 1 · abort · strip | 21,692,416 | 16,506,656 | 4,812,552 | 11,270,656 | 8,313,104 | — |
| **A** | 패키지별 `s`(tokio · tokio-postgres · tiberius · oracledb · rusqlite · libsqlite3-sys · regex 3 · fancy-regex · wasmi · wasmparser · serde · winit · nsql-drivers + driver 5 · `--config` 인자) | 20,154,368(**−1.47 MB · −7.1 %**) | 15,079,088 | 4,721,272 | 10,044,928(−1.17 MB) | 7,195,280 | 8m24s |
| **B** | 전역 `s`(`CARGO_PROFILE_RELEASE_OPT_LEVEL=s`) | 16,126,464(**−5.31 MB · −25.7 %**) | 11,441,712 | 4,327,528 | 8,299,520(−2.83 MB) | 5,811,520 | 5m42s |
| F | 전역 `s` + 핫 패스 3(nexa-gfx · nexa-ctl · nexa-font · nsql-core · nsql-run) | (대기) | | | | | |
| C | 전역 `z` | (대기) | | | | | |
| D | `panic=unwind` | (대기) | | | | | |
| E | A + `--remap-path-prefix` | (대기) | | | | | |

성능(A·B·F · 기동 3회 중앙값 · nexa-ui `bench_editor` 70만 줄 입력 · 결과 그리드 10만 행 페치/스크롤 1회) = (대기)

### 5-3. DBMS 아이콘(측정 3)

- **PNG 692장 = 1,602,822 B(≈1.53 MiB)** — §2의 "3.3 MB"는 과대(실제는 그 절반) · 173 id × 4칸(light/dark × 64/20) + `NOTICE.md` 5,317 B.
- 칸별 = dark/64 690,280 B · light/64 595,532 · dark/20 161,700 · light/20 149,993 · 분포 = 최소 450 · 중앙 1,243 · 최대 9,599 B(<1 KiB 301 · 1~2 KiB 58 · 2~4 KiB 217 · ≥4 KiB 117) · 상위 = lmdb 9,599/9,344 · singlestore 6,772 · snowflake 6,737 · firebolt 5,624 · pinecone 5,610 · graphite · citus 5,565 · kylin 5,527 · victoriametrics 5,463.
- 내장 방식 = `crates/nexa-sql/build.rs:10-42` `gen_dbms_icons` → `OUT_DIR/dbms_icons_table.rs` `static TABLE: &[(&str, [&[u8]; 4])]`(칸마다 `include_bytes!` · 빠지면 `assert!` 빌드 실패) · 실행 = `dbms_icons.rs` 이진 탐색 · 첫 사용 때 디코드(스레드 로컬 캐시) · 모르는 id = `generic` · 디코드 실패 = 1×1 투명.
- **S-1 절감 재산정 = ≈ −1.5 MB**(종전 추정 −3.0) · 64 px 칸만 외부화하면 −1.29 MB · 20 px 칸만 남기면 exe 0.31 MB.
- "exe 옆 파일 하나" 배포 변경 자리:

| 패키징 | 파일:줄 | 변경 |
|---|---|---|
| 공통 스테이징(deb · mac .app) | `packaging/lib.sh:49-58` `stage_common()` | `install -m 0644 … dbms-icons.pack` 한 줄 |
| Windows MSI 스테이징 | `packaging/windows/build-msi.ps1:98-103` | `Copy-Item <pack> $Stage`(lib.sh를 안 씀) |
| WiX | `packaging/windows/nexa-sql.wxs:75-99` `MainFiles` | 새 `<Component>`(새 GUID) · 하위 폴더면 64-69 `<Directory>` |
| rpm | `packaging/linux/nexa-sql.spec:47-62` `%files` | `%{_datadir}/nexa-sql/dbms-icons.pack`(빠지면 "installed but unpackaged" 실패) |
| deb | `packaging/linux/build-deb.sh:54-55` | 없음(stage_common) · 주석만 |
| mac .app/pkg | `build-app.sh:63`(자동 · 서명은 stage 뒤 순서 확인) · `build-pkg.sh:75-79` 검사(선택) | — |
| dmg · Homebrew cask · choco · winget | — | 없음 |
| release.yml | deb/rpm 19-23·141-142 · mac 59-64·178-183 · MSI 90-93·209-212 | pack 생성이 cargo 밖이면 각 잡에 생성 단계 · 스모크 검사 4(선택) |
| 고지 | `scripts/third-party-notices.py:83-90` | 원본 경로를 옮기면 84행만 |
| ★ 런타임 경로 | (없음) | 설치 폴더 자원 해석 함수 새로 = Win exe 옆 · mac `../Resources` · Linux `../share/nexa-sql` · 개발 빌드 폴백 |
| 문서 | 33 §2 34-44(Win 37 · mac 42 · Linux 44) · §3 76~ · §5-4 177-178 | 레이아웃 갱신 |

### 5-4. TLS 단일화(측정 4)

- 지금 = **MSSQL** `nsql-driver-mssql` → tiberius 0.12.3 → tokio-rustls 0.24.1 → rustls 0.21.12 → **ring 0.17.14**(+ rustls-webpki 0.101.7 · sct 0.7.1 · rustls-native-certs 0.6.3 · rustls-pemfile 1.0.4) / **Oracle Thin** oracledb =26.0.0-beta.4 → rustls 0.23.45 → **aws-lc-rs 1.18.2 → aws-lc-sys 0.46.0**(C 빌드) · rustls-webpki 0.103.15 · webpki-roots 1.0.9 / **PG** = postgres 0.19 → tokio-postgres 0.7.18 · **TLS 없음**(`NoTls` · nsql-driver-pg/src/lib.rs:8·24·240·712 · T-70) · native-tls/openssl 없음.
- 기능 = tiberius `default-features=false, ["tds73","rustls","chrono"]` + winauth · oracledb 기능 = `arrow`뿐 · rustls를 기본 기능으로 의존 → `aws_lc_rs` 늘 켜짐 · 저장소 코드에 `ClientConfig` 직접 생성 0건(oracledb `transport.rs:496·563` `builder()` = 프로세스 기본 공급자).
- 상류 = tiberius 최신 **0.13.0**(2026-09-25) = rustls 0.23 + tokio-rustls 0.26(기본 기능 = `aws_lc_rs`) · oracledb = ring 공급자 기능 **없음** · rustls 기능은 합집합이라 아래에서 `aws_lc_rs`를 못 끈다 · ⚠ ring + aws_lc_rs 동시 켜짐 = `CryptoProvider::from_crate_features()` None → `builder()` panic(`install_default` 필요).
- **경로 1 = tiberius 0.13(aws-lc 통일) ✅ 가능** — 빠짐 = rustls 0.21 · ring · sct · webpki 0.101 · tokio-rustls 0.24 · pemfile 1 · 고칠 곳 = `crates/nsql-driver-mssql/Cargo.toml:22` · `src/lib.rs:945-1012` Config(**`command_timeout(None)` 필수** = 0.13 기본 30 s · handshake 15 s) · bulk `ColumnData`(:334-375) · tracing PRINT(:31-33) 확인 · 위험 = timeout · encrypt 기본 Required · secrecy · thiserror 2 · 재검증 = Attention 취소 · TDS bulk 13.9k행/s · stop-latency · grid-edit/bulk E2E · 절감 ⚠ **0.5~1.0 MB**(큰 aws-lc가 남음 · 맵 표로 확정).
- **경로 2 = oracledb를 ring으로 ❌ 상류 그대로 불가** — 포크면 2-a(oracledb 포크 · rustls 두 판 남음 · ⚠ 1~2 MB) / 2-b(tiberius 0.13 + oracledb·tokio-rustls 포크 · ring 하나 · ⚠ 1.5~2.5 MB · 포크 2~3개 유지 · beta 드라이버 · PQ 키 교환 상실).
- → D-331 "ring 통일 우선"은 근거 소멸 · 포크 없이면 **경로 1이 유일**.
- 출처 = crates.io/api/v1/crates/tiberius · …/tiberius/0.13.0/dependencies · docs.rs/crate/tiberius/0.13.0/features · raw.githubusercontent.com/prisma/tiberius/main/CHANGELOG.md · crates.io/api/v1/crates/tokio-rustls/0.26.4 · crates.io/api/v1/crates/oracledb · docs.rs/rustls/0.23.45/rustls/crypto/struct.CryptoProvider.html · 로컬 레지스트리 원본(oracledb-26.0.0-beta.4/Cargo.toml:33-37·295-296 · rustls-0.23.45/src/crypto/mod.rs:265-286).

### 5-5. 10-22 cargo-bloat 측정(.text · 협업 · 135차 win)

- 조건 = cargo-bloat 0.12.1 · `CARGO_TARGET_DIR=target/bloat` · `CARGO_PROFILE_RELEASE_STRIP=none` · Release 재빌드(GUI 13m51s · CLI 3m24s) · 기준 = HEAD 25dea18(134차 커밋본 · 소스 변경 0). **MSVC는 디버그 정보가 .pdb로 가므로 strip=none이어도 exe 크기 = 배포본과 같음** — dumpbin 섹션: GUI `.text` 16,518,144 · `.rdata` 4,809,216 · `.pdata` 241,152 · `.reloc` 62,464 · `.rsrc` 35,840 · `.data` 32,256 = 20.7 MB(배포 `target/release` 16,523,776 · 4,816,896 · ±0.1 %) / CLI `.text` 8,189,440 · `.rdata` 2,676,224 · `.pdata` 156,160 = 10.6 MB(배포 8,322,560 · 2,692,608). cargo-bloat는 **.text만** 세고 추정치(도구의 Note 그대로) · `.rdata`(아이콘·문자열·표)는 §5-1 맵 측정(10-21)에만 있다.
- 상위 15 크레이트(.text) **GUI**: nexa_sql 4.0 MiB · std 2.3 · (빈 이름 = C 코드) 1.6 · nexa_ctl 591 KiB · rustls 499(**0.21+0.23 합산 · 도구가 나누지 못함**) · wast 460 · regex_automata 397 · wasmparser 354 · nsql_script 331 · nsql_catalog 330 · wasmi 309 · oracledb 253 · nsql_run 192 · aws_lc_sys 190 · hashbrown 189. **CLI**: C 코드 1.6 MiB · std 1.1 · rustls 500 KiB · nsql_catalog 308 · regex_automata 304 · nsql 273 · oracledb 253 · aws_lc_sys 194 · tiberius 178 · nsql_script 177 · regex_syntax 140 · nsql_run 135 · aho_corasick 131 · nsql_i18n 131 · nsql_format 128.
- C 코드 1.6 MiB 분해(심볼 접두 · 집계 스크립트 = 협업 scratchpad `bloat_agg.py` · 부록 §A 재생성 절차) = sqlite3 **0.92**(`sqlite3Fts*`·fts3/5 심볼 다수 = libsqlite3-sys bundled 기본 ENABLE) · aws-lc 식별 0.28(`aws_lc_0_*` 0.20 + ML-KEM/ML-DSA 0.07 + jent 0.01) · 미분류 0.59(md·pkey·cipher·common_ = 대부분 aws-lc 추정 · 일부 sqlite) · odpi 0.10 · **ring C/asm 0.02**(44 심볼).
- 소계(GUI / CLI) = **(a) ring 스택** = ring 0.08/0.07 + tokio_rustls·pemfile 소량 + ring C 0.02 → ≈0.1 MiB + rustls 0.21·webpki 0.101 몫(합산 0.49+0.06 중 미상 · 상한 ≈0.3) → **tiberius 0.13 승격의 .text 절감 ≈ 0.1~0.4 MiB** · **(b) aws-lc 스택** = aws_lc_sys 0.19 + C 0.28~0.87 → ≈0.5~0.9 MiB + rustls 0.23 몫 · **(c-1) WASM** = wast+wasmparser+wasmi+wasmi_ir **1.16 MiB**(GUI만 · CLI 0 · `wast` 460 KiB = wasmi `default` → feature `wat`(텍스트 형식 파서) · `cargo tree -i wast` = wast ← wat ← wasmi "wat" ← wasmi "default" · Cargo.toml `wasmi = "1.1"`) · **(c-2) regex** = regex_automata+regex_syntax+aho_corasick+fancy_regex 0.71/0.57 · **(c-3) encoding_rs** 0.06 · 그 밖 = sqlite3 C+sys 0.98 · std 2.34/1.07 · odpi+oracle 0.24 · Thin(oracledb) 0.31 · tiberius+mssql 0.29 · PG 0.19~0.22. Release 프로필은 이미 `opt-level 3 · lto fat · codegen-units 1 · panic abort · strip symbols`.
- cargo tree(두 rustls 판의 경로) = rustls 0.21.12 ← tokio-rustls 0.24.1(`dangerous_configuration`) ← tiberius 0.12.3(`chrono`·`rustls`) ← nsql-driver-mssql / rustls 0.23.45 ← oracledb 26.0.0-beta.4 ← nsql-driver-oracle-thin · ring 역의존 = rustls 0.21뿐 · aws-lc-rs 역의존 = rustls 0.23뿐.
- ★ **두 측정의 차이 메모(10-21 맵 vs 10-22 cargo-bloat)** = 맵(§5-1) = **전 섹션**(.text + .rdata 등) · 심볼 소속으로 집계 · **두 rustls 판을 나눠** ring 0.42 + rustls 0.52 / cargo-bloat(§5-5) = **.text만** · rustls 두 판 **합산** 0.49 · ring 스택 식별분 0.1(+몫 상한 0.3) · C 코드는 접두 추정. → TLS 단일화(손잡이 3)의 절감은 **범위로 표기 = 하한 0.1~0.4 MB(bloat · .text만) ~ 상한 0.6~0.8 MB(맵 · 전 섹션 · §6 표)** · 결론 = 어느 쪽이든 아이콘 외부화(−1.5)·최적화 수준(−4~5)·wat 제거(−0.5~0.6)보다 작다 · 승격의 주된 가치는 크기보다 **보안 권고 해소·덤 패닉 제거**(§5-6).

### 5-6. tiberius 0.13 승격 사전 검토(개발 10-22 · 변경 로그 대조 · 착수 = T-382 때)

- **BREAKING 3** = (1) `command_timeout` 기본 **30 s** · `handshake_timeout` **15 s**(종전 무제한) → 우리 Config에 `command_timeout(None)`(또는 `Session::set_call_timeout` 값) **명시 필수**(안 하면 긴 질의·bulk가 30 s에 끊김) (2) encrypt 기본 **Required** → 우리는 `config.encryption(...)`을 늘 명시(`crates/nsql-driver-mssql/src/lib.rs:976`) → 영향 0 (3) 자격 = `secrecy::SecretString` → `AuthMethod::sql_server` 생성자만 써서 영향 0.
- 그 밖 = `trust_cert_ca` 누적 규칙(우리는 `trust_cert()`만 · 959행) · async-std 기능 삭제 = 미사용 · 덤 = 디코드 패닉 → 오류 반환(T-309 `todo!()` 부류 해소) · 핸드셰이크 멈춤 감지 · 보안 권고 해소 · thiserror 2.
- 검증(착수 때 · 협업 V1·V2) = BISCM_MS_DEV(192.168.0.58:1433/M4PLAN_MS · 10-22 등록) 실서버 = grid-edit MSSQL · conn-cmd 38 · tdsbulk(13.9k행/s 기준) · 동일성 E2E · encrypt on/off 접속 · Attention 취소·stop-latency · Linux·mac = CI `integration`(SQL Server 컨테이너) + 세션 실기 1회.

## 6. 결정안(측정 1·3·4 반영 · 성능(F·B 벤치)은 §5-2 대기)

**묶음 요약(GUI 맵 합계 21.17 MB · §5-1)** = 앱 본체 4.16(20 %) · 무명 상수·문자열 3.70(17 % · 아이콘 1.60 포함) · 표준 라이브러리 단형화 2.67(13 %) · TLS·암호 2.35(11 % = aws-lc C 1.30 · rustls 0.52 · ring 0.42) · SQLite C 1.51(7 %) · nsql 코어 1.46(7 %) · WASM 1.28(6 % · 그중 `wast` 0.50) · UI 1.09(5 %) · regex 0.78(4 %) · Oracle 0.65 · SQL Server 0.31 · PG 0.25 · encoding_rs 0.20.

| 순서 | 손잡이 | 절감(GUI · 실측/추정) | 조건 | 결정 |
|---|---|---|---|---|
| 1 | **최적화 수준** = 변형 **F**(전역 `s` + 핫 패스 5크레이트 `3`) 또는 B(전역 `s`) | B −5.31 MB(−25.7 % 실측) · F = B와 A(−1.47) 사이 추정 −4~5 MB | §5-2 벤치(편집기 70만 줄 · 기동 · 10만 행 페치)에서 회귀 ≤ 5 %인 가장 작은 쪽 | **D-332** = F 기본 · F가 모자라면 B |
| 2 | **아이콘 외부화**(S-1) | **−1.5 MB**(692장 1.60 MB) | pack 파일 1 + generic 폴백 · 패키징 4종 수정 | **D-330** ✅ |
| 3 | **TLS 스택 단일화**(S-2 · §7) | **−0.6~0.8 MB**(ring 0.42 + rustls 0.21) | 길 = tiberius 0.13(aws-lc 통일 · `command_timeout` None) · `ring` 통일 불가 | **D-331** = aws-lc 통일 · 새 드라이버(MySQL)는 **같은 rustls 0.23 + aws-lc 공급자 공유**(TLS 추가분 ≈ 0 · 협업 기준) · OS 네이티브 TLS(§7-3)는 **정책 옵션**(`vendored` 금지 · 동적 링크만 · 크기 효과는 같음) |
| 4 | **`wasmi` `wat` 끄기**(S-13) | **−0.5~0.6 MB** | 확장 = `.wasm`만(규칙 75) | **D-336a** ✅ 즉시 |
| 5 | regex·tokio 피처 정리(S-5·S-8) · remap(S-9) | −0.3~0.5 MB | 찾기·필터 회귀 시험 | **D-333** 조건부 ✅ |
| — | 합계 | **−7~8.5 MB → GUI ≈ 13~14.5 MB(−35~40 %)** · CLI ≈ 6.5~7.5 MB | | |
| ⏸ | 드라이버 cdylib 분리(S-11) | 설치본 총량 불변 | 121 §7 = Tibero·DuckDB만 확장 | **D-334** 보류 유지 |
| 측정 | `panic = "unwind"`(S-12 · D-329) | 변형 D 뒤 | | **D-335** 대기 |

**드라이버가 늘 때 크기를 지키는 규칙(MySQL·ODBC 내장 D-342 대비)** = ① 새 드라이버의 TLS는 **이미 있는 rustls 0.23 + aws-lc 공급자 공유**(추가분 ≈ 0 · native-tls/openssl `vendored`·C 클라이언트(libmysqlclient) 금지 = +1~3 MB) ② 드라이버·프로토콜 crate는 `opt-level = "s"`(패키지별 · 핫 패스 아님) ③ 순수 Rust 드라이버만 내장(C 번들이 큰 것 = 확장 · DuckDB) ④ 기능 플래그 최소(`default-features = false`) ⑤ 인벤토리(71 A) 전/후 측정을 드라이버 추가 체크리스트(108 §10)에 넣는다.

## 7. 암호화(TLS) 로직의 어댑터 추상화 · 플러그인 적재 · 미사용 회수 — 가능성 검토(사용자 질문 10-21)

> 질문: "암호화 로직을 어댑터 방식으로 추상화하고, 방식별로 라이브러리(플러그인) 형식으로 외부에서 로딩·사용하고, 미사용 시 메모리를 회수하도록 구현할 수 있나?"
> 답: **부분적으로만 가능하고, 가장 큰 효과는 "OS 네이티브 TLS를 쓰는 것"에서 나온다.** 이유와 설계를 아래에.

### 7-1. 사실 — 암호화는 우리 코드가 아니라 드라이버 안에 있다

| 드라이버 | TLS가 어디서 결정되나 | 지금 | 바꿀 수 있는 손잡이 |
|---|---|---|---|
| SQL Server `tiberius 0.12` | **컴파일 타임 기능 플래그** `rustls` / `native-tls` / `vendored-openssl` · TDS PRELOGIN 안에서 드라이버가 직접 핸드셰이크(스트림을 밖에서 감쌀 수 없음) | `rustls`(= rustls 0.21 + **ring**) | 플래그를 `native-tls`로 → Windows SChannel · macOS Security.framework · Linux OpenSSL(동적) = **ring·rustls 0.21 삭제** |
| Oracle Thin `oracledb 26.0.0-beta.4` | crate 내부 고정 = rustls 0.23 + **aws-lc-rs**(포크 없이는 못 바꿈 · 협업 §5-4) | aws-lc(C 코드 정적) | 없음(Thin을 쓰는 한 aws-lc는 남음) |
| PostgreSQL `postgres`(동기) | **포트**(`MakeTlsConnect` trait) = 호출자가 넣는다 | `NoTls`(T-70 미구현) | 우리 어댑터를 그대로 꽂을 수 있음 = `postgres-native-tls` 또는 rustls |
| SQLite · OCI | TLS 없음 / Instant Client 안 | — | — |
| MySQL(P1 예정) · ODBC(P2 예정) | `mysql` crate = 기능 플래그(`native-tls` / `rustls`) · ODBC = OS 드라이버 관리자 안 | — | 처음부터 `native-tls` |

→ "어댑터 추상화"는 **드라이버가 TLS 포트를 열어 준 곳(PG · 앞으로 MySQL)에서만** 우리 trait로 가능하고, tiberius·oracledb는 **컴파일 타임 선택** 또는 **포크**뿐이다.

### 7-2. "플러그인으로 외부 로딩 · 미사용 회수"가 되는 것과 안 되는 것

| 방식 | 가능? | 설명 |
|---|---|---|
| **OS 네이티브 TLS**(SChannel · Security.framework · OpenSSL) | ✅ **이미 그 모델** | 암호 라이브러리가 **OS 공유 DLL**이라 exe에 0 바이트 · OS가 첫 사용 때 적재하고 페이지를 관리(미사용 페이지 회수 = OS 몫) · 인증서 저장소·사내 CA·FIPS 정책이 OS 설정을 따름(기업 환경 장점) · 비용 = Linux는 배포판 `libssl3` 의존(deb/rpm 의존 선언 · 33) |
| rustls `CryptoProvider`(ring / aws-lc)를 **런타임 플러그인**으로 | ✗ | rustls 0.23의 공급자는 런타임 선택이지만 **Rust 제네릭·불안정 ABI**라 cdylib로 분리할 수 없음 · 공급자 코드는 어차피 링크됨 |
| **드라이버 전체를 cdylib 확장으로**(121 §7) | ✅(적재) · ⚠(해제) | 드라이버가 자기 TLS를 동봉 → exe에서 분리 · 첫 접속 때 적재 · **해제(FreeLibrary/dlclose)는 권장 안 함** = Rust cdylib에 전역 상태(rustls 공급자 · aws-lc C 전역 · tokio 런타임 스레드 · thread-local)가 남아 해제 뒤 크래시 위험 → "첫 사용 때 적재 · 프로세스 끝까지 유지"가 안전한 규칙 · **완전 회수가 꼭 필요하면 격리 프로세스 모드**(`nsql-driver-rpc` · 프로세스 종료 = 0) |
| 우리 자체 TLS 어댑터 + 외부 암호 cdylib(C ABI · 예 OpenSSL `libcrypto`) | ⚠ 과함 | TLS 프로토콜 구현을 직접 쓰는 것과 같음(rustls가 이미 그 역할) · 보안 책임 증가 · 효과는 네이티브 TLS와 같음 |

### 7-3. 권장 설계(D-343)

1. **TLS 포트 `TlsPort`**(nsql-core · 의존 0) = `connect(host, stream, opts{verify · ca · sni · min_version}) → TlsStream` · 구현 = ① `tls-native`(기본 · OS) ② `tls-rustls`(선택 · 사내 정책으로 OS 스택을 못 쓸 때) — **드라이버가 포트를 받는 곳(PG · MySQL)에 주입**, tiberius는 기능 플래그 `native-tls`로 같은 스택을 쓰게.
2. **효과** = ring + rustls 0.21 제거(**−1~1.5 MB** · 측정 5-1 뒤 확정) · TLS 스택 = aws-lc(Thin 전용) + OS 네이티브(그 밖) · Thin을 확장 드라이버로 빼면 aws-lc도 exe에서 빠진다(그러나 Thin = 기본 Oracle 경로 D-278 · 내장 유지 권장 → aws-lc ≈ 2.5~3 MB는 남음 · 수치는 5-1).
3. **적재·회수 규칙** = 내장 드라이버의 TLS = OS 공유 라이브러리(exe 0 · OS 관리) · 확장 드라이버 = 첫 접속 적재 · 프로세스 끝까지 유지 · 완전 회수 필요 시 격리 프로세스 모드(121 §7-4).
4. **검증** = SQL Server encrypt=true(M4PLAN) · PG `sslmode=require`(Supabase · T-70 동시 해결) · 3-OS 접속 E2E · Linux deb/rpm `libssl3` 의존 · 사내 CA(OS 저장소) 시나리오.
5. **영향** = 기동 0(OS DLL은 첫 TLS 접속 때 적재) · 메모리 = OS 공유 페이지(여러 프로세스가 공유) · 보안 갱신 = OS 패치로(장점).

| # | 결정 | 권장 |
|---|---|---|
| D-343 | TLS = `TlsPort` 추상화(PG·MySQL 주입 자리) · **기본 스택 = rustls 0.23 + aws-lc 하나로 통일**(Thin이 강제하므로 · tiberius 0.13 · 새 드라이버 공유 → 크기 추가 0) · **OS 네이티브**(SChannel · Security.framework · OpenSSL 동적)는 정책 옵션(기업 CA·FIPS · 크기 효과 동일 · `vendored` 금지) · 확장 드라이버는 자기 TLS 동봉 + 해제 없음(격리 모드로 회수) | ✅ 권장(사용자 확정 대기) · 협업 측정 1 기준으로 "aws-lc 공유"를 기본으로 수정 |

### 7-4. 영향 확인(사용자 질문 10-21 "지금 Win/mac/Linux에서 Oracle·MSSQL·PG·SQLite가 잘 되는데 권장안으로 가도 문제없나?")

| DB | 지금 TLS | 안 ① rustls + aws-lc 통일(tiberius 0.13 · **권장**) | 안 ② OS 네이티브 기본(native-tls) |
|---|---|---|---|
| Oracle OCI | Instant Client 안(우리 코드 밖) | **영향 0** | 영향 0 |
| Oracle Thin | rustls 0.23 + aws-lc(고정) | **영향 0**(그대로) | 영향 0(못 바꿈) |
| SQLite | 없음 | 영향 0 | 영향 0 |
| **SQL Server** | tiberius 0.12 + rustls 0.21 + ring | **동작 동일한 범위** = 같은 rustls 계열(TLS 1.2/1.3 · 인증서 검증 규칙 같음 · `trust_server_certificate` 그대로) · 바뀌는 것 = tiberius 0.12 → **0.13 API 차이**(`command_timeout` 기본 30 s → `None` 필수 · 시그니처 변경 점검) → **실서버 E2E로 확인**(M4PLAN encrypt · grid-edit 96 · conn-cmd 38 · tdsbulk 28 · 3-OS CI) | Windows = SChannel · mac = Security.framework · **Linux = OpenSSL 동적 링크 → 런타임 `libssl` 의존이 새로 생김**(배포판 soname 차이 libssl1.1/3 · RHEL8·구 배포판에서 **적재 실패 위험** · deb/rpm 의존 선언 필요) · 자체 서명 인증서(SQL Server 기본) = OS 검증 규칙으로 바뀜(`trust_cert` 필요 · mac Security.framework는 일부 자체 서명·약한 해시 거부 사례) |
| **PostgreSQL** | `NoTls`(TLS 미지원 · T-70) | TLS 추가 = **개선만**(rustls 0.23 + aws-lc 공유 · `sslmode=require` 지원 · Supabase) | 개선이지만 Linux `libssl` 의존 동일 |
| 3-OS 빌드 | Linux = cmake·clang·nasm 없이 aws-lc 빌드 ✓(124차 실측) | **그대로**(이미 되는 빌드) | Linux CI 이미지의 OpenSSL 헤더·버전에 묶임 |
| 크기 | — | **−0.6~0.8 MB**(ring + rustls 0.21 제거) | 같음(−0.6~0.8 · Windows SChannel crate는 작음) |

**결론** = 안 ①은 "지금 되는 것은 그대로 되고(Oracle·SQLite 0 · SQL Server는 같은 rustls 계열 · PG는 개선), 바뀌는 것은 tiberius 한 판 올림뿐"이라 **실서버·3-OS E2E만 통과하면 사용에 문제없다**. 안 ②는 Windows/mac은 무난하나 **Linux 이식성(libssl 런타임 의존)과 자체 서명 인증서 정책 변화**가 회귀 위험 → **기본으로 쓰지 않고 정책 옵션**으로만. 따라서 D-343 = 안 ①.
**안 ① 검증 계획(개발 뒤 협업 V1·V2)** = Windows: M4PLAN encrypt=on/off 접속 · `win-grid-edit` MSSQL · conn-cmd 38 · bulk tdsbulk · 동일성 E2E · Linux·mac: CI `integration`(SQL Server 컨테이너) + 맥·리눅스 세션 실기 1회 · 회귀 기준 = 지금과 같은 결과.

## 부록 A. 10-22 cargo-bloat 원자료(협업 · 작은 파일 6 · 전체 함수 목록 2.5 MB·1.3 MB는 제외)

> 재생성 = `$env:CARGO_TARGET_DIR="target/bloat"; $env:CARGO_PROFILE_RELEASE_STRIP="none"; cargo bloat --release -p nexa-sql --crates -n 60` · `… -p nsql-cli --crates -n 60` · 함수 전수 = `… -p nexa-sql -n 0 > gui-fns-all.txt`(CLI 같음) · 접두 집계 = 협업 scratchpad `bloat_agg.py`(빈 크레이트명 = C 코드 → sqlite3/aws_lc/ring/odpi 접두 분류) · 산출물 폴더 `target/bloat/`(자체 관리 · 지워도 됨).

### A-1. GUI 크레이트별(`cargo bloat --release -p nexa-sql --crates -n 60`)

```text
    Analyzing target/bloat\release\nexa-sql.exe
 File  .text     Size Crate
19.1%  25.1%   4.0MiB nexa_sql
11.3%  14.9%   2.3MiB std
 7.9%  10.3%   1.6MiB
 2.8%   3.7% 590.8KiB nexa_ctl
 2.4%   3.1% 498.9KiB rustls
 2.2%   2.9% 459.9KiB wast
 1.9%   2.5% 397.2KiB regex_automata
 1.7%   2.2% 354.0KiB wasmparser
 1.6%   2.1% 330.7KiB nsql_script
 1.6%   2.0% 329.6KiB nsql_catalog
 1.5%   1.9% 309.4KiB wasmi
 1.2%   1.6% 253.0KiB oracledb
 0.9%   1.2% 192.0KiB nsql_run
 0.9%   1.2% 190.3KiB aws_lc_sys
 0.9%   1.2% 189.3KiB hashbrown
 0.9%   1.1% 181.0KiB nexa_dlg
 0.8%   1.1% 178.1KiB tiberius
 0.7%   0.9% 141.6KiB regex_syntax
 0.6%   0.8% 132.3KiB [Unknown]
 0.6%   0.8% 130.9KiB aho_corasick
 0.6%   0.8% 130.8KiB enum2$<nsql_i18n
 0.6%   0.8% 129.9KiB winit
 0.6%   0.8% 128.0KiB nsql_format
 0.6%   0.8% 123.5KiB nsql_driver_mssql
 0.6%   0.7% 117.9KiB tokio_postgres
 0.5%   0.6%  97.9KiB odpic_sys
 0.4%   0.6%  93.2KiB nexa_gfx
 0.4%   0.5%  78.7KiB ttf_parser
 0.4%   0.5%  78.0KiB nsql_driver_oracle
 0.4%   0.5%  78.0KiB ring
 0.3%   0.4%  72.3KiB aes
 0.3%   0.4%  68.6KiB nsql_driver_pg
 0.3%   0.4%  65.0KiB libsqlite3_sys
 0.3%   0.4%  63.3KiB nsql_driver_oracle_thin
 0.3%   0.4%  61.8KiB nsql_core
 0.3%   0.4%  60.8KiB wasmi_ir
 0.3%   0.4%  60.7KiB sha2
 0.3%   0.4%  60.7KiB oracle
 0.3%   0.4%  59.3KiB webpki
 0.3%   0.4%  59.1KiB enum2$<encoding_rs
 0.3%   0.3%  54.4KiB fancy_regex
 0.2%   0.3%  51.4KiB nsql_io
 0.2%   0.3%  50.9KiB pkcs5
 0.2%   0.3%  49.8KiB chrono
 0.2%   0.3%  43.2KiB nsql_settings
 0.2%   0.2%  39.7KiB nsql_bookmarks
 0.2%   0.2%  39.7KiB nsql_log
 0.2%   0.2%  36.5KiB nexa_fs
 0.2%   0.2%  35.5KiB tokio
 0.1%   0.2%  31.3KiB curve25519_dalek
 0.1%   0.2%  29.3KiB nsql_drivers
 0.1%   0.2%  28.8KiB nsql_vault
 0.1%   0.2%  26.0KiB postgres_protocol
 0.1%   0.2%  25.4KiB nsql_search
 0.1%   0.1%  22.1KiB bytes
 0.1%   0.1%  21.4KiB nexa_license
 0.1%   0.1%  19.9KiB nsql_driver_sqlite
 0.1%   0.1%  19.0KiB enum2$<wast
 0.1%   0.1%  18.2KiB memchr
 0.1%   0.1%  16.6KiB nexa_font
 1.7%   2.2% 354.7KiB And 123 more crates. Use -n N to show more.
76.1% 100.0%  15.8MiB .text section size, the file size is 20.7MiB
Note: numbers above are a result of guesswork. They are not 100% correct and never will be.
```

### A-2. CLI 크레이트별(`cargo bloat --release -p nsql-cli --crates -n 60`)

```text
    Analyzing target/bloat\release\nsql.exe
 File  .text     Size Crate
15.3%  20.8%   1.6MiB
10.1%  13.7%   1.1MiB std
 4.6%   6.3% 500.1KiB rustls
 2.8%   3.9% 308.2KiB nsql_catalog
 2.8%   3.8% 304.2KiB regex_automata
 2.5%   3.4% 273.1KiB nsql
 2.3%   3.2% 252.9KiB oracledb
 1.8%   2.4% 193.8KiB aws_lc_sys
 1.6%   2.2% 178.4KiB tiberius
 1.6%   2.2% 176.8KiB nsql_script
 1.3%   1.8% 140.4KiB regex_syntax
 1.2%   1.7% 134.9KiB nsql_run
 1.2%   1.6% 131.0KiB aho_corasick
 1.2%   1.6% 130.8KiB enum2$<nsql_i18n
 1.2%   1.6% 127.5KiB nsql_format
 1.1%   1.6% 124.0KiB nsql_driver_mssql
 1.1%   1.5% 119.4KiB [Unknown]
 1.1%   1.5% 118.1KiB tokio_postgres
 0.9%   1.2%  98.0KiB odpic_sys
 0.8%   1.1%  86.2KiB nsql_driver_oracle
 0.7%   0.9%  74.6KiB ring
 0.7%   0.9%  72.3KiB aes
 0.6%   0.9%  68.9KiB nsql_driver_pg
 0.6%   0.8%  65.0KiB libsqlite3_sys
 0.6%   0.8%  64.4KiB nsql_driver_oracle_thin
 0.6%   0.8%  60.7KiB sha2
 0.6%   0.8%  60.1KiB oracle
 0.5%   0.7%  59.3KiB webpki
 0.5%   0.7%  58.8KiB enum2$<encoding_rs
 0.5%   0.6%  50.9KiB pkcs5
 0.5%   0.6%  49.8KiB chrono
 0.4%   0.6%  48.3KiB nsql_io
 0.4%   0.6%  46.9KiB nsql_settings
 0.4%   0.5%  41.5KiB hashbrown
 0.3%   0.4%  35.5KiB tokio
 0.3%   0.4%  34.1KiB nsql_core
 0.3%   0.4%  33.6KiB nsql_search
 0.3%   0.4%  31.3KiB curve25519_dalek
 0.3%   0.4%  30.9KiB nsql_log
 0.2%   0.3%  26.1KiB postgres_protocol
 0.2%   0.3%  24.7KiB nexa_license
 0.2%   0.3%  24.4KiB nsql_vault
 0.2%   0.3%  22.1KiB bytes
 0.2%   0.3%  21.1KiB nsql_bookmarks
 0.2%   0.2%  19.9KiB nsql_driver_sqlite
 0.2%   0.2%  19.7KiB nsql_drivers
 0.1%   0.2%  15.6KiB postgres_types
 0.1%   0.2%  14.5KiB tokio_rustls
 0.1%   0.2%  14.4KiB memchr
 0.1%   0.2%  14.1KiB postgres
 0.1%   0.2%  13.3KiB der
 0.1%   0.2%  12.7KiB aws_lc_rs
 0.1%   0.1%  11.2KiB rustls_pki_types
 0.1%   0.1%  11.2KiB base64ct
 0.1%   0.1%  10.3KiB poly1305
 0.1%   0.1%  10.3KiB enum2$<oracledb
 0.1%   0.1%   9.9KiB encoding_rs
 0.1%   0.1%   9.4KiB rusqlite
 0.1%   0.1%   8.5KiB nsql_license
 0.1%   0.1%   8.3KiB rustls_pemfile
 1.2%   1.6% 126.2KiB And 85 more crates. Use -n N to show more.
73.6% 100.0%   7.8MiB .text section size, the file size is 10.6MiB
Note: numbers above are a result of guesswork. They are not 100% correct and never will be.
```

### A-3. GUI 접두 집계(C 코드 분류 · `bloat_agg.py gui-fns-all.txt`)

```text
rows=18214
   3.95 MiB  nexa_sql
   2.34 MiB  std
   0.92 MiB  <sqlite3>
   0.59 MiB  <unattributed>
   0.58 MiB  nexa_ctl
   0.49 MiB  rustls
   0.45 MiB  wast
   0.39 MiB  regex_automata
   0.35 MiB  wasmparser
   0.32 MiB  nsql_script
   0.32 MiB  nsql_catalog
   0.30 MiB  wasmi
   0.25 MiB  oracledb
   0.19 MiB  nsql_run
   0.19 MiB  aws_lc_sys
   0.19 MiB  hashbrown
   0.18 MiB  nexa_dlg
   0.17 MiB  tiberius
   0.14 MiB  regex_syntax
   0.13 MiB  enum2$<nsql_i18n
   0.13 MiB  aho_corasick
   0.13 MiB  winit
   0.12 MiB  nsql_format
   0.12 MiB  nsql_driver_mssql
   0.12 MiB  tokio_postgres
   0.10 MiB  <aws_lc>
   0.10 MiB  odpic_sys
   0.09 MiB  [Unknown]
   0.09 MiB  nexa_gfx
   0.08 MiB  ttf_parser
   0.08 MiB  ring
   0.08 MiB  nsql_driver_oracle
   0.07 MiB  aes
   0.07 MiB  nsql_driver_pg
   0.06 MiB  libsqlite3_sys
   0.06 MiB  nsql_driver_oracle_thin
   0.06 MiB  nsql_core
   0.06 MiB  wasmi_ir
   0.06 MiB  sha2
   0.06 MiB  oracle
   0.06 MiB  webpki
   0.06 MiB  enum2$<encoding_rs
   0.05 MiB  fancy_regex
   0.05 MiB  nsql_io
   0.05 MiB  pkcs5
```

### A-4. CLI 접두 집계(`bloat_agg.py cli-fns-all.txt`)

```text
rows=10256
   1.07 MiB  std
   0.92 MiB  <sqlite3>
   0.58 MiB  <unattributed>
   0.49 MiB  rustls
   0.30 MiB  nsql_catalog
   0.30 MiB  regex_automata
   0.27 MiB  nsql
   0.25 MiB  oracledb
   0.19 MiB  aws_lc_sys
   0.17 MiB  tiberius
   0.17 MiB  nsql_script
   0.14 MiB  regex_syntax
   0.13 MiB  nsql_run
   0.13 MiB  enum2$<nsql_i18n
   0.13 MiB  aho_corasick
   0.12 MiB  nsql_format
   0.12 MiB  nsql_driver_mssql
   0.12 MiB  tokio_postgres
   0.10 MiB  <aws_lc>
   0.10 MiB  odpic_sys
   0.08 MiB  nsql_driver_oracle
   0.08 MiB  [Unknown]
   0.07 MiB  ring
   0.07 MiB  aes
   0.07 MiB  nsql_driver_pg
   0.06 MiB  libsqlite3_sys
   0.06 MiB  nsql_driver_oracle_thin
   0.06 MiB  sha2
   0.06 MiB  oracle
   0.06 MiB  webpki
   0.06 MiB  enum2$<encoding_rs
   0.05 MiB  pkcs5
   0.05 MiB  chrono
   0.05 MiB  nsql_io
   0.05 MiB  nsql_settings
   0.04 MiB  hashbrown
   0.03 MiB  tokio
   0.03 MiB  nsql_core
   0.03 MiB  nsql_search
   0.03 MiB  curve25519_dalek
   0.03 MiB  nsql_log
   0.03 MiB  <std/compiler-rt>
   0.03 MiB  <odpi(dpi)>
   0.03 MiB  postgres_protocol
   0.02 MiB  nexa_license
```

### A-5. GUI 함수 단위 상위 40(`-n 0` 결과의 머리)

```text
 0.9%   1.2% 188.3KiB                                          nexa_sql nexa_sql::app::event_loop::impl$0::about_to_wait
 0.7%   0.9% 152.9KiB                                          nexa_sql nexa_sql::App::paint
 0.6%   0.8% 130.8KiB                                  enum2$<nsql_i18n enum2$<nsql_i18n::Msg>::row
 0.6%   0.7% 117.5KiB                                          nexa_sql nexa_sql::main
 0.5%   0.6% 101.0KiB                                          nexa_sql nexa_sql::App::drain_events
 0.4%   0.6%  92.1KiB                                    regex_automata regex_automata::meta::strategy::new
 0.4%   0.5%  80.5KiB                                          nexa_sql nexa_sql::explorer::meta_thread
 0.4%   0.5%  76.6KiB                                          nexa_sql nexa_sql::explorer::Explorer::drain
 0.3%   0.3%  55.0KiB                                          wasmi_ir wasmi_ir::enum::impl$5::fmt
 0.2%   0.3%  50.4KiB                                              wast wast::core::expr::impl$46::parse::closure$0
 0.2%   0.3%  49.9KiB                                          nexa_ctl nexa_ctl::controls::textbox::TextBox::paint_multiline
 0.2%   0.3%  49.3KiB                                          nexa_sql nexa_sql::App::aux_window_event
 0.2%   0.3%  47.1KiB                                          nexa_sql nexa_sql::App::drain_conn
 0.2%   0.3%  46.5KiB                                          nexa_dlg nexa_dlg::FilePicker::open_menu
 0.2%   0.3%  46.0KiB                                          nexa_sql nexa_sql::intel::Intel::request
 0.2%   0.3%  43.5KiB                                          nexa_sql nexa_sql::App::startup_cmd
 0.2%   0.2%  40.0KiB                                          nexa_dlg nexa_dlg::FilePicker::new
 0.2%   0.2%  37.3KiB                                               std core::ptr::drop_glue<nexa_sql::App>
 0.2%   0.2%  37.1KiB                                          nexa_sql nexa_sql::grid::Grid::paint_inner
 0.2%   0.2%  36.8KiB                                             pkcs5 pkcs5::pbes2::encryption::decrypt_in_place<alloc::vec::Vec<u8,alloc::alloc::Global> >
 0.2%   0.2%  35.7KiB                                                   sqlite3VdbeExec
 0.2%   0.2%  35.4KiB                                          tiberius tiberius::client::connection::impl$1::connect::async_fn$0<tokio_util::compat::Compat<tokio::net::tcp::stream::TcpStream> >
 0.2%   0.2%  35.2KiB                                          nexa_sql nexa_sql::worker::spawn::closure$0::closure$7
 0.2%   0.2%  34.4KiB                                          nexa_sql nexa_sql::conn_win::ConnWin::paint
 0.2%   0.2%  33.2KiB                                          tiberius tiberius::tds::stream::token::impl$0::try_unfold::closure$0::async_block$0<tokio_util::compat::Compat<tokio::net::tcp::stream::TcpStream> >
 0.2%   0.2%  33.1KiB                                             wasmi wasmi::engine::executor::instrs::execute_instrs
 0.2%   0.2%  32.4KiB                                enum2$<encoding_rs enum2$<encoding_rs::variant::VariantEncoder>::encode_from_utf8_raw
 0.1%   0.2%  31.4KiB                                      nsql_catalog nsql_catalog::gen::object_ddl
 0.1%   0.2%  30.7KiB                                          nexa_sql nexa_sql::App::apply_extensions
 0.1%   0.2%  30.6KiB                                              wast wast::core::binary::encode
 0.1%   0.2%  30.5KiB                                          nexa_sql nexa_sql::App::explorer_actions
 0.1%   0.2%  29.4KiB                                          oracledb oracledb::client::Client::connect
 0.1%   0.2%  29.2KiB                                          nexa_sql nexa_sql::App::build_menus_with
 0.1%   0.2%  28.6KiB                                      nsql_drivers nsql_drivers::open
 0.1%   0.2%  27.9KiB                                          oracledb oracledb::transport::CustomClientCertResolver::populate
 0.1%   0.2%  27.5KiB                                    nsql_driver_pg nsql_driver_pg::PgSession::execute_inner
 0.1%   0.2%  26.2KiB                                      nsql_catalog nsql_catalog::source_with
 0.1%   0.2%  26.1KiB                                             wasmi wasmi::module::parser::ModuleParser::parse_module
 0.1%   0.2%  25.7KiB                                enum2$<encoding_rs enum2$<encoding_rs::variant::VariantDecoder>::decode_to_utf8_raw
 0.1%   0.2%  25.1KiB                                 nsql_driver_mssql nsql_driver_mssql::MssqlSession::execute_inner
```

### A-6. CLI 함수 단위 상위 40(`cargo bloat --release -p nsql-cli -n 40`)

```text
    Analyzing target/bloat\release\nsql.exe
 File  .text     Size                   Crate Name
 1.2%   1.6% 130.8KiB        enum2$<nsql_i18n enum2$<nsql_i18n::Msg>::row
 0.8%   1.0%  83.6KiB                    nsql nsql::main
 0.7%   1.0%  79.3KiB          regex_automata regex_automata::meta::strategy::new
 0.3%   0.5%  36.8KiB                   pkcs5 pkcs5::pbes2::encryption::decrypt_in_place<alloc::vec::Vec<u8,alloc::alloc::Global> >
 0.3%   0.4%  35.7KiB                         sqlite3VdbeExec
 0.3%   0.4%  35.5KiB                tiberius tiberius::client::connection::impl$1::connect::async_fn$0<tokio_util::compat::Compat<tokio::net::tcp::stream::TcpStream> >
 0.3%   0.4%  33.3KiB                tiberius tiberius::tds::stream::token::impl$0::try_unfold::closure$0::async_block$0<tokio_util::compat::Compat<tokio::net::tcp::stream::TcpStream> >
 0.3%   0.4%  32.4KiB      enum2$<encoding_rs enum2$<encoding_rs::variant::VariantEncoder>::encode_from_utf8_raw
 0.3%   0.4%  31.6KiB            nsql_catalog nsql_catalog::gen::object_ddl
 0.3%   0.4%  29.5KiB                oracledb oracledb::client::Client::connect
 0.3%   0.4%  28.3KiB                nsql_run nsql_run::Runner::execute
 0.3%   0.4%  28.0KiB                nsql_run nsql_run::Runner::perform
 0.3%   0.3%  27.9KiB                oracledb oracledb::transport::CustomClientCertResolver::populate
 0.3%   0.3%  27.7KiB          nsql_driver_pg nsql_driver_pg::PgSession::execute_inner
 0.2%   0.3%  26.4KiB            nsql_catalog nsql_catalog::source_with
 0.2%   0.3%  25.7KiB      enum2$<encoding_rs enum2$<encoding_rs::variant::VariantDecoder>::decode_to_utf8_raw
 0.2%   0.3%  25.7KiB                    nsql nsql::cat::cmd_cat::closure$0
 0.2%   0.3%  25.1KiB       nsql_driver_mssql nsql_driver_mssql::MssqlSession::execute_inner
 0.2%   0.3%  24.2KiB                     std std::sys::process::windows::Command::spawn_with_attributes
 0.2%   0.3%  22.8KiB nsql_driver_oracle_thin nsql_driver_oracle_thin::impl$1::execute
 0.2%   0.3%  22.4KiB             nsql_format nsql_format::layout::layout
 0.2%   0.3%  21.9KiB                nsql_run nsql_run::Runner::import_file
 0.2%   0.3%  20.9KiB            nsql_catalog nsql_catalog::gen::generate
 0.2%   0.3%  20.9KiB                    nsql nsql::cmd_shell
 0.2%   0.3%  20.5KiB                         sqlite3Pragma
 0.2%   0.2%  19.7KiB                tiberius tiberius::tds::codec::column_data::impl$0::decode::async_fn$0<tiberius::client::connection::Connection<tokio_util::compat::Compat<tokio::net::tcp::stream::TcpStream> > >
 0.2%   0.2%  19.5KiB          regex_automata regex_automata::nfa::thompson::compiler::Compiler::c
 0.2%   0.2%  18.9KiB            nsql_drivers nsql_drivers::open
 0.2%   0.2%  18.1KiB                    nsql nsql::Printer::handle
 0.2%   0.2%  18.1KiB            nsql_catalog nsql_catalog::detail::object_details
 0.2%   0.2%  17.7KiB             nsql_script nsql_script::engine::Engine::plan_exec
 0.2%   0.2%  17.7KiB      nsql_driver_oracle nsql_driver_oracle::impl$5::execute
 0.2%   0.2%  17.6KiB                nsql_run nsql_run::Runner::run_item
 0.2%   0.2%  17.1KiB                  rustls rustls::conn::ConnectionCommon<rustls::client::client_conn::ClientConnectionData>::complete_io<rustls::client::client_conn::ClientConnectionData,std::net::tcp::TcpStream>
 0.2%   0.2%  17.1KiB                  rustls rustls::client::hs::emit_client_hello_for_retry
 0.2%   0.2%  16.8KiB          tokio_postgres tokio_postgres::connect::connect_once::async_fn$0<tokio_postgres::tls::NoTls>
 0.2%   0.2%  16.7KiB      nsql_driver_oracle nsql_driver_oracle::OracleSession::connect
 0.2%   0.2%  16.5KiB                  rustls rustls::msgs::handshake::impl$89::encode
 0.1%   0.2%  16.0KiB                         yy_reduce
 0.1%   0.2%  15.6KiB          regex_automata regex_automata::meta::regex::Builder::build
59.2%  80.5%   6.3MiB                         And 10216 smaller methods. Use -n N to show more.
73.6% 100.0%   7.8MiB                         .text section size, the file size is 10.6MiB
```
