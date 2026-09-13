# 20 · 실서버 테스트 — GitHub Codespaces · 각 DBMS별 Docker · Actions 통합 워크플로

> 사용자(09-12): *"github에서 codespaces를 사용한 테스트 구성"* · *"각 DBMS별 docker 방식으로"*. D-14의 답 → **DR-21**.

## 1. 구성 한눈에

```text
.devcontainer/
  devcontainer.json     Codespaces/VS Code 진입 — dev 서비스 · 기본 기동 oracle+mssql · ★ 최소 사양 2코어/8GB/32GB · NSQL_*_URL 주입 · CARGO_BUILD_JOBS=1
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
| **Codespaces** | 저장소 → Code → Codespaces → Create(기본 2-core/8GB로 충분 — 느리지만 동작 · 사용자 09-13). 생성 후 DB가 healthy 되면 `scripts/it.sh` · 또는 `nsql run -c "$NSQL_ORACLE_URL" examples/it-oracle.sql` |
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
- ★ 최소 사양(2-core/8GB)에 맞춰 컨테이너 메모리 상한을 둔다: oracle 2g · mssql 1.5g(`MSSQL_MEMORY_LIMIT_MB=1024`) · pg 512m · mysql 768m, Rust 빌드 병렬 1. 첫 빌드는 10분 안팎, 메모리가 모자라면 `docker compose -f .devcontainer/docker-compose.yml stop mssql`로 안 쓰는 DB를 내린다(docker-outside-of-docker 기능 포함).
- Instant Client는 OTN 라이선스 — 개발 컨테이너 내 사용이며 제품 배포 동봉은 D-1.

## 5. macOS에 Oracle Instant Client 설치·설정 (전 패키지 한 폴더 · CLI 포함)

> Windows 절차(`7z x instantclient*.zip -o C:\Oracle`)의 macOS 대응. **자동 설치**: `scripts/install-instantclient-mac.sh` (아래 §5-8). 수동 절차는 5-1~5-7.
> ★ macOS의 `sqlplus`는 `LC_RPATH = @executable_path/` 라서 **모든 패키지를 한 폴더에 풀면 라이브러리를 알아서 찾는다**(`DYLD_LIBRARY_PATH` 불필요 — SIP 때문에 어차피 자식 프로세스에 전달되지 않는다). 실측: `otool -l sqlplus | grep -A2 LC_RPATH`.

### 5-0. 폴더 위치·이름 추천

| 후보 | 평가 |
|---|---|
| ★ **`~/Oracle/instantclient_19_16`** + 링크 **`~/Oracle/instantclient`** | **권장**. Windows의 `C:\Oracle\instantclient_23_6`와 같은 모양 · sudo 불필요 · macOS 업그레이드에 안전 · 버전 폴더를 나란히 두고 링크만 바꿔 전환 |
| `/opt/oracle/instantclient_19_16` | Oracle 문서 관례 · 여러 사용자 공유 · `sudo` 필요. 팀 공용 Mac이면 이쪽 |
| `/usr/local/oracle/…` | Intel Mac에서 Homebrew 영역과 섞인다 — 피할 것 |
| `~/Library/Oracle/…` | Finder에서 숨겨져 CLI·문제 해결이 불편 |

규칙: **폴더 이름에 버전을 넣고(`instantclient_<major>_<minor>`), 버전 없는 심볼릭 링크(`instantclient`)를 하나 만들어 `~/.zshrc`는 링크만 가리킨다.** 새 버전은 옆에 풀고 링크만 다시 걸면 끝이다.

```sh
export INSTANT_CLIENT_ROOT="$HOME/Oracle"
export INSTANT_CLIENT_VER="19.16.0.0.0"        # Apple Silicon이면 23.26.2.0.0
export INSTANT_CLIENT_PATH="$INSTANT_CLIENT_ROOT/instantclient"
```

### 5-1. 어떤 본을 받나

| Mac | 최신 버전 | 배포 형식 | 비고 |
|---|---|---|---|
| **Intel x86_64** (이 Mac) | **19.16.0.0.0** | **DMG만** (버전 경로에 zip 없음) | 19.16이 Intel 마지막 본. Oracle 19c~23ai 서버에 접속된다 |
| Apple Silicon arm64 | 23.26.2.0.0 | **ZIP**(버전 경로) · DMG | |

⚠️ 버전 없는 링크(`…/instantclient-basic-macos.zip`)는 **19.8로 낡아 있다**(실측 2026-09-13). 버전 경로를 쓴다.

| 패키지 | Intel 19.16 (DMG) | Apple Silicon 23.26.2 (ZIP) |
|---|---|---|
| Basic | `…/1916000/instantclient-basic-macos.x64-19.16.0.0.0dbru.dmg` | `…/2326200/instantclient-basic-macos.arm64-23.26.2.0.0.zip` |
| SQL*Plus | `…/1916000/instantclient-sqlplus-macos.x64-…dbru.dmg` | `…/2326200/instantclient-sqlplus-macos.arm64-….zip` |
| Tools(`sqlldr`·`expdp`·`impdp`) | `…instantclient-tools-macos.x64-…dbru.dmg` | `…instantclient-tools-macos.arm64-….zip` |
| SDK | `…instantclient-sdk-macos.x64-…dbru.dmg` | `…instantclient-sdk-macos.arm64-….zip` |
| ODBC | `…instantclient-odbc-macos.x64-…dbru.dmg` | `…instantclient-odbc-macos.arm64-….zip` |
| JDBC | `…instantclient-jdbc-macos.x64-…dbru.dmg` | `…instantclient-jdbc-macos.arm64-….zip` |

베이스 URL은 `https://download.oracle.com/otn_software/mac/instantclient/`. 목록 페이지: [Intel](https://www.oracle.com/database/technologies/instant-client/macos-intel-x86-downloads.html) · [ARM64](https://www.oracle.com/database/technologies/instant-client/macos-arm64-downloads.html).
(참고: Windows 정리 표의 SDK 행이 `jdbc` URL, JDBC 행이 `odbc` URL로 잘못 들어가 있다 — 같이 고치면 좋다.)

### 5-2. 다운로드

```sh
mkdir -p ~/Downloads/ic && cd ~/Downloads/ic
BASE=https://download.oracle.com/otn_software/mac/instantclient/1916000
for p in basic sqlplus tools sdk odbc jdbc; do
  curl -fLO "$BASE/instantclient-$p-macos.x64-19.16.0.0.0dbru.dmg"
done
```

### 5-3. 한 폴더에 풀기 (Windows의 `7z x … -o C:\Oracle -aoa`)

**Intel — DMG 마운트 후 복사**(Oracle의 `install_ic.sh`는 `~/Downloads/instantclient_19_16`로 경로가 박혀 있어 쓰지 않는다):
```sh
DEST="$HOME/Oracle/instantclient_19_16"; mkdir -p "$DEST"
for f in ~/Downloads/ic/*.dmg; do
  MP=$(hdiutil attach -nobrowse -readonly "$f" | awk '/\/Volumes\//{ $1=""; $2=""; sub(/^ +/,""); print; exit}')
  cp -R -P -p -f "$MP"/* "$DEST"/
  hdiutil detach "$MP" -quiet
done
rm -f "$DEST/install_ic.sh" "$DEST/INSTALL_IC_README.txt"
ln -sfn "$DEST" "$HOME/Oracle/instantclient"
```
**Apple Silicon — ZIP**(zip 안에 `instantclient_23_26/` 한 겹이 있다):
```sh
DEST="$HOME/Oracle/instantclient_23_26"; mkdir -p "$DEST"
for f in ~/Downloads/ic/*.zip; do unzip -qo "$f" -d /tmp/ic && cp -R -P -p -f /tmp/ic/instantclient_*/* "$DEST"/; done
rm -rf /tmp/ic; ln -sfn "$DEST" "$HOME/Oracle/instantclient"
```

### 5-4. ★ 격리 속성 해제 (macOS 필수 — Windows에 없는 단계)

```sh
xattr -dr com.apple.quarantine "$HOME/Oracle/instantclient_19_16"
```
안 하면 *"확인되지 않은 개발자"* 로 `sqlplus` 실행과 dylib 로드가 막힌다.

### 5-5. PATH·환경변수 (`~/.zshrc` — Windows의 `SetEnvironmentVariable("Path"…)`)

```sh
cat >> ~/.zshrc <<'EOF'

# Oracle Instant Client (nexa-sql)
export INSTANT_CLIENT_PATH="$HOME/Oracle/instantclient"
export PATH="$INSTANT_CLIENT_PATH:$PATH"
export TNS_ADMIN="$INSTANT_CLIENT_PATH/network/admin"
export NLS_LANG=KOREAN_KOREA.AL32UTF8
export NSQL_ORACLE_CLIENT_DIR="$INSTANT_CLIENT_PATH"
EOF
source ~/.zshrc
echo $PATH | tr ':' '\n' | head -3      # 확인(Windows: $Env:Path -split ';')
```
⚠️ `DYLD_LIBRARY_PATH`는 쓰지 않는다 — SIP가 자식 프로세스에서 지운다.
★ **GUI 앱(nexa-sql 창)은 셸 환경변수를 못 받는다** → `~/lib` 링크를 같이 만든다(ODPI-C 기본 탐색 경로):
```sh
mkdir -p ~/lib && ln -sf "$HOME/Oracle/instantclient"/libclntsh*.dylib* ~/lib/ && ln -sf "$HOME/Oracle/instantclient"/libnnz*.dylib ~/lib/
```

### 5-6. 연결 정보 등록 — `$TNS_ADMIN/tnsnames.ora`

```sh
mkdir -p "$INSTANT_CLIENT_PATH/network/admin"
cat > "$INSTANT_CLIENT_PATH/network/admin/tnsnames.ora" <<'EOF'
M4PLAN55 =
  (DESCRIPTION =
    (ADDRESS = (PROTOCOL = TCP)(HOST = 192.168.0.55)(PORT = 1521))
    (CONNECT_DATA = (SERVER = DEDICATED)(SERVICE_NAME = XE))
  )

M4PLAN58 =
  (DESCRIPTION =
    (ADDRESS = (PROTOCOL = TCP)(HOST = 192.168.0.58)(PORT = 1521))
    (CONNECT_DATA = (SERVER = DEDICATED)(SERVICE_NAME = BISCM))
  )
EOF
```

### 5-7. 연결 테스트 (Windows와 동일한 3형식)

```sh
sqlplus -V                                            # 설치 확인
sqlplus BISCM/biscm@192.168.0.55:1521/XE              # EZConnect
sqlplus BISCM/biscm@M4PLAN55                          # tnsnames 별칭
sqlplus BISCM/biscm@"(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST=192.168.0.55)(PORT=1521))(CONNECT_DATA=(SERVER=DEDICATED)(SERVICE_NAME=XE)))"
# nexa-sql CLI — 같은 접속을 우리 도구로
nsql run -c "oracle://BISCM:biscm@192.168.0.55:1521/XE" examples/it-oracle.sql
```
(선택) 방향키 히스토리: `brew install rlwrap` 후 `alias sqlplus='rlwrap sqlplus'`.

### 5-8. 한 줄 설치 (이 저장소 스크립트)

```sh
scripts/install-instantclient-mac.sh            # ~/Oracle 에 6개 패키지 · 격리 해제 · ~/lib·버전 링크까지
scripts/install-instantclient-mac.sh --rc       # ~/.zshrc 설정까지 자동
INSTANT_CLIENT_ROOT=/opt/oracle scripts/install-instantclient-mac.sh basic sqlplus
```
아키텍처를 보고 Intel은 DMG, Apple Silicon은 ZIP을 받는다. **실측(이 Mac · Intel i9 · macOS 26)**: 6패키지 설치 255MB · `sqlplus -V` → `Release 19.0.0.0.0 / Version 19.16.0.0.0` · `nsql run`이 DPI-1047 없이 `ORA-12541`(서버 없음)까지 도달 = 클라이언트 로드 성공.

### 5-9. 한글 깨짐

macOS 터미널은 기본이 UTF-8이라 Windows의 `[Console]::OutputEncoding` 같은 단계가 없다. `NLS_LANG`만 맞추면 된다.
```sh
export NLS_LANG=KOREAN_KOREA.AL32UTF8     # 서버 NLS_CHARACTERSET 이 AL32UTF8 일 때
locale                                     # LANG=ko_KR.UTF-8 또는 en_US.UTF-8 이면 정상
```
서버 문자셋 확인: `SELECT VALUE FROM NLS_DATABASE_PARAMETERS WHERE PARAMETER = 'NLS_CHARACTERSET';`
서버가 `KO16MSWIN949`면 `export NLS_LANG=KOREAN_KOREA.KO16MSWIN949`.
