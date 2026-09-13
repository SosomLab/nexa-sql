# 11 · CLI 도구 `nsql` — 접속 · 스크립트 · export/import · bulk insert

> 사용자 요구(09-12): *"CLI 도구도 개발 범위에. 각 OS의 CLI에서 데이터 접속 및 기능 사용, export/import 도구 및 bulk insert 등 핵심 기능. 데이터베이스의 핵심 기능을 쓰려면 드라이버 구현이 필요."*
> 원칙: **GUI와 CLI는 같은 코어를 쓴다**(`nsql-core` · `nsql-script` · 드라이버 · `nsql-io`). CLI가 먼저 완성되는 것이 자연스럽다 — 화면 없이 드라이버·엔진을 실기 검증할 수 있고, [03 §6](03-competitive-landscape.md)에서 "GUI+TUI를 한 코어에서 내는 제품은 사실상 없음"이 차별점으로 확인됐다.

## 1. 명령 체계 (제안)

```text
nsql plan     [-d d] <script|-> [args]              # ✅ 엔진 dry-run(실행 없이 계획 출력)
nsql run      -c <연결> [-f fmt] <script|-> [args]  # ✅ 스크립트 실행(&1..&n · WHENEVER · 프롬프트)
nsql shell    -c <연결>                             # ✅ 최소 대화형(줄 누적 · `;`/`/`/명령으로 실행) · ☐ 줄 편집·히스토리
nsql export   -c <연결> (-q sql | -t table) -f csv|tsv|json|jsonl|insert[:T] [-o file]   # ✅ · ☐ parquet/xlsx
nsql import   -c <연결> --table t --from file [--format …] [--mode insert|upsert|replace] [--batch 5000]
nsql bulk     -c <연결> --table t --from file       # ★ 방언별 네이티브 대량 적재(§3)
nsql desc     -c <연결> <object>                    # DESCRIBE
nsql conn     list|add|show|rm|test|path              # ✅ 연결 프로필(사용자 폴더 암호화 저장 · GUI와 공유 · [21](21-connection-profiles.md))
```

- `-c <연결>` = ✅ 프로필 이름 또는 접속 문자열(`oracle://u@h:1521/svc` · `mssql://…` · `u/p@tns`). 스크립트 `CONNECT <이름>`도 프로필을 쓴다.
- 종료 코드·`WHENEVER SQLERROR EXIT`·stdin 파이프·`--format json` 출력으로 **스크립트/CI 친화**.
- sqlplus 호환 별칭 `nsql sqlplus u/p@db @file.sql` · sqlcmd 호환 `nsql sqlcmd -S -U -P -i` — 기존 배치 파일 교체 비용 0을 노린다.

## 2. 드라이버 계층 (`nsql-driver-*`) — [06 A](06-rust-ecosystem.md)

| 순위 | DBMS | 크레이트 | 인증·비고 |
|---|---|---|---|
| 1급 | **Oracle** | 지금 `oracle`(kubo · ODPI-C · Instant Client 런타임 로드) → 공식 순수 Rust `oracledb` GA 시 교체 | REF CURSOR OUT · DBMS_OUTPUT · LOB · 배열 바인딩 |
| 1급 | **SQL Server** | `tiberius-ng` 또는 `tiberius`(재개) → Microsoft `mssql-tds` 성숙 시 | SSPI/Kerberos/Entra 토큰 · `sp_executesql` OUTPUT · BulkLoad |
| 2급 | PostgreSQL / MySQL / SQLite | `tokio-postgres` / `mysql_async` / `rusqlite(bundled)` | COPY · LOAD DATA · 트랜잭션 배치 |
| 3급 | Tibero · Altibase · CUBRID · 기타 | `odbc-api` 폴백 | 벤더 클라이언트 사용자 설치 |
| 확장 | JDBC 전용 DB 등 | stdio JSON-RPC 드라이버 플러그인([09 §4-3](09-editor-and-packages.md)) | 언어 무관 |

모든 드라이버는 `nsql_core::Session` 포트(`execute` · `fetch_cursor` · `commit` · `rollback`) + 카탈로그 포트(오브젝트 브라우저용)를 구현한다. 동기 드라이버는 워커 스레드에서 돌리고 UI/CLI는 채널로 받는다(clip DR-41 "UI 스레드는 기다리지 않는다").

## 3. Bulk insert — 방언별 네이티브 경로 (`nsql-io`)

| DBMS | 경로 | 비고 |
|---|---|---|
| Oracle | ODPI-C `executeMany`(배열 바인딩) · 선택적 `APPEND` 힌트 · 옵션 direct-path | SQL*Loader 대체 목표 |
| SQL Server | TDS `BulkLoadRequest`(tiberius) = `BULK INSERT`/bcp 동급 | 테이블 락·배치 크기 |
| PostgreSQL | `COPY … FROM STDIN`(binary/text) | 최고속 |
| MySQL | `LOAD DATA LOCAL INFILE` 또는 다중 행 `INSERT` 배치 | `local_infile` 권한 |
| SQLite | 트랜잭션 + prepared 반복 | |
| ODBC | 컬럼 배열 바인딩(`odbc-api` bulk) | |

공통: 스트리밍 읽기(CSV/TSV/JSONL/Parquet) → 타입 변환(방언 규칙) → 배치 → 오류 행 격리 파일 + 재시작 오프셋. export는 결과 스트림을 같은 포맷 작성기로 — GUI "내보내기"와 코드 공유.

## 4. 단계

1. **M1**: `nsql run/shell` + Oracle·MSSQL 드라이버 + `export csv/json` — 엔진 실기 검증.
2. **M2**: `import/bulk` 6방언 · PG/MySQL/SQLite 드라이버 · ODBC 폴백.
3. **M3**: ~~연결 프로필 키체인 공유~~(✅ 09-13 DR-22 · 파일 저장소) · sqlplus/sqlcmd 호환 별칭 · Parquet/XLSX.
