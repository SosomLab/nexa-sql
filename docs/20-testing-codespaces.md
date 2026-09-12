# 20 · 실서버 테스트 — GitHub Codespaces · 각 DBMS별 Docker · Actions 통합 워크플로

> 사용자(09-12): *"github에서 codespaces를 사용한 테스트 구성"* · *"각 DBMS별 docker 방식으로"*. D-14의 답 → **DR-21**.

## 1. 구성 한눈에

```text
.devcontainer/
  devcontainer.json     Codespaces/VS Code 진입 — dev 서비스 · 기본 기동 oracle+mssql · 4코어/16GB 요구 · NSQL_*_URL 주입
  docker-compose.yml    ★ DBMS마다 컨테이너 1개: oracle(gvenzl/oracle-free:23-slim-faststart) · mssql(SQL Server 2022 Developer) ·
                        postgres:17 · mysql:8.4 (뒤 둘은 프로필 pg/mysql — 드라이버 M4 후)
  Dockerfile            Rust stable + Oracle Instant Client basiclite(영구 링크) + 한글 폰트(Noto CJK·나눔코딩)
  post-create.sh        형제 저장소 ../nexa-ui clone(path 의존) · cargo fetch · DB 대기
scripts/wait-for-db.sh  포트 대기(환경변수 있는 DB만)
scripts/it.sh           통합 테스트 실행
crates/nsql-drivers/tests/integration.rs   ★ 환경변수 게이트 테스트 5건(없으면 [skip])
examples/it-oracle.sql · it-mssql.sql        nsql run 실기 스크립트(블록 EXEC · REFCURSOR · DBMS_OUTPUT · :setvar/&치환 · GO)
.github/workflows/integration.yml           같은 컨테이너를 서비스로 띄우는 자동 검증(main push · PR · 수동)
```

## 2. 사용법

| 어디서 | 어떻게 |
|---|---|
| **Codespaces** | 저장소 → Code → Codespaces → Create(머신 4-core 이상). 생성 후 DB가 healthy 되면 `scripts/it.sh` · 또는 `nsql run -c "$NSQL_ORACLE_URL" examples/it-oracle.sql` |
| 로컬 Docker | `docker compose -f .devcontainer/docker-compose.yml up -d oracle mssql` → `ORACLE_HOST=localhost MSSQL_HOST=localhost NSQL_ORACLE_URL=oracle://nexa:nexa@localhost:1521/FREEPDB1 NSQL_MSSQL_URL='mssql://sa:Nexa%40Sql2026@localhost:1433/master' scripts/it.sh`(Instant Client는 로컬에 설치 · `LD_LIBRARY_PATH`/`DYLD_LIBRARY_PATH`) |
| GitHub Actions | push마다 `integration` 워크플로 — 결과는 Actions 탭 · `gh run watch` |
| PG/MySQL | `docker compose --profile pg --profile mysql up -d` · 드라이버는 M4(T-23) |

접속 정보(테스트 전용 · 컨테이너 안에서만 유효): Oracle `nexa/nexa@…/FREEPDB1`(관리자 `SYSTEM/Nexa_Sql2026`) · SQL Server `sa/Nexa@Sql2026`(URL에서는 `%40`).

## 3. 통합 테스트가 확인하는 것

| 테스트 | Oracle | SQL Server |
|---|---|---|
| 세션 변수 왕복 | `EXEC :V := …` 로컬 · `SELECT … INTO :V_CNT` 블록 EXEC → OUT 회수 · `PRINT` · 후속 조회 바인드 | 같은 스크립트가 `SELECT @V_CNT = …` + 트레일러로 회수 |
| REF CURSOR | `VARIABLE rc REFCURSOR` → `OPEN :rc FOR` → `PRINT rc` 1회 fetch | (해당 없음 — 결과 집합) |
| DBMS_OUTPUT | `SET SERVEROUTPUT ON` → `PUT_LINE` 메시지 수신 | — |
| PL/SQL 블록 OUT | `BEGIN … :V_TWICE := :V_NM || :V_NM; END; /` | — |
| DML·배치 | CREATE/INSERT/COMMIT/DROP | `#temp` · `GO` 배치 · rows affected · 한글 NVARCHAR |

## 4. 주의
- gvenzl Oracle Free는 amd64 이미지 — Apple Silicon 로컬은 Rosetta 에뮬레이션(느림) · Codespaces(amd64)가 편하다.
- SQL Server 컨테이너는 메모리 2GB 이상 필요 → Codespaces 2-core/8GB에서는 두 DB 동시 기동이 빠듯해 4-core/16GB를 `hostRequirements`로 요구.
- Instant Client는 OTN 라이선스 — 개발 컨테이너 내 사용이며 제품 배포 동봉은 D-1.
