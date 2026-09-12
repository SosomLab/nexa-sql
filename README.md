# Nexa SQL

**3-OS(Windows · macOS · Linux)에서 똑같이 생긴, 가벼운 SQL 클라이언트(IDE) + CLI `nsql`.**
전부 Rust · 단일 실행 파일 · 자체 CPU 래스터라이저([nexa-ui](../nexa-ui)) · Qt·WebView·Electron 없음.

- ★ **SQL*Plus식 세션 변수를 모든 DBMS에서** — `EXEC :V := 'x'` · `SELECT … INTO :V` · `PRINT` · REFCURSOR · 편집기 안 `connect user/pass@host:1521/db;`. SQL Server에서도 다수 문장이 같은 변수를 본다(`sp_executesql` OUTPUT 재작성).
- Oracle · SQL Server 1급, PostgreSQL · MySQL · SQLite 2급, Tibero · Altibase · CUBRID(ODBC) 3급 — 단일 앱, 방언 계층.
- 편집기는 Sublime Text 관습, 확장은 Sublime 패키지 구조(`.sublime-syntax` 등 그대로).

## 지금 할 수 있는 것 (M1·M2 진행 중)

```sh
# 계획만(DB 불요)
cargo run -p nsql-cli -- plan --dialect mssql examples/golden-session-vars.sql
# SQLite로 세션 변수 왕복 실기
cargo run -p nsql-cli -- run -c sqlite::memory: examples/sqlite-session-vars.sql
# Oracle / SQL Server (드라이버 구현 완료 · 실서버 검증 대기)
cargo run -p nsql-cli -- run -c "oracle://user:pass@host:1521/svc" script.sql
cargo run -p nsql-cli -- run -c "mssql://user:pass@host:1433/db" script.sql
cargo run -p nsql-cli -- export -c sqlite:app.db -t emp -f json -o emp.json
cargo run -p nsql-cli -- shell -c sqlite::memory:
# GUI 최소 창(접속 · 편집 · ⌘/Ctrl+Enter 실행 · 그리드)
cargo run -p nexa-sql -- sqlite::memory:
```
실서버 검증은 GitHub Codespaces(각 DBMS Docker 컨테이너) 또는 `integration` 워크플로로 — [docs/20](docs/20-testing-codespaces.md). Oracle은 Instant Client가 런타임에 필요하다(공식 순수 Rust 드라이버 GA 시 교체 예정). 편집기는 아직 임시(`TextBox`) — 정식 편집기 계획은 [docs/17](docs/17-editor-incremental-plan.md).

설계·조사·진행 기록은 [`docs/`](docs/) — 시작은 [docs/00 기반 보고서](docs/00-foundation-report.md).

## 라이선스

[PolyForm Noncommercial 1.0.0](LICENSE.md) — 개인·비상업 무료, 상업적 사용은 별도 라이선스. ([한국어](LICENSE.ko.md))
