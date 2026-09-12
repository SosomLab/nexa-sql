# Nexa SQL

**3-OS(Windows · macOS · Linux)에서 똑같이 생긴, 가벼운 SQL 클라이언트(IDE) + CLI `nsql`.**
전부 Rust · 단일 실행 파일 · 자체 CPU 래스터라이저([nexa-ui](../nexa-ui)) · Qt·WebView·Electron 없음.

- ★ **SQL*Plus식 세션 변수를 모든 DBMS에서** — `EXEC :V := 'x'` · `SELECT … INTO :V` · `PRINT` · REFCURSOR · 편집기 안 `connect user/pass@host:1521/db;`. SQL Server에서도 다수 문장이 같은 변수를 본다(`sp_executesql` OUTPUT 재작성).
- Oracle · SQL Server 1급, PostgreSQL · MySQL · SQLite 2급, Tibero · Altibase · CUBRID(ODBC) 3급 — 단일 앱, 방언 계층.
- 편집기는 Sublime Text 관습, 확장은 Sublime 패키지 구조(`.sublime-syntax` 등 그대로).

## 지금 할 수 있는 것 (M0)

```sh
cargo run -p nsql-cli -- plan --dialect oracle examples/golden-session-vars.sql
cargo run -p nsql-cli -- plan --dialect mssql  examples/golden-session-vars.sql
```
드라이버 없이 스크립트를 항목으로 나누고, 변수·바인드·방언 재작성 계획을 출력한다. 실접속은 M1.

설계·조사·진행 기록은 [`docs/`](docs/) — 시작은 [docs/00 기반 보고서](docs/00-foundation-report.md).

## 라이선스

[PolyForm Noncommercial 1.0.0](LICENSE.md) — 개인·비상업 무료, 상업적 사용은 별도 라이선스. ([한국어](LICENSE.ko.md))
