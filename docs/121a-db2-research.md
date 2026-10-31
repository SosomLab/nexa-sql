# 121a — Db2 LUW 조사 원문 (10-25 · 138차 Linux)

> [121 데이터베이스 지원 우선순위](121-database-support-priority.md)와 **D-359**(Db2 = 내장 ODBC 어댑터 위 전용 방언 `Dialect::Db2` · 별도 크레이트·확장 id 없음 · 벤더 clidriver는 사용자 설치)의 근거 자료. 협업 세션(Opus)이 하위 에이전트 3개로 조사 = ①② CI 컨테이너·clidriver · ③⑤ SYSCAT 카탈로그 + DBeaver 대조(워크트리 초안 질의 **검증**) · ④ 오류·문법·EXPLAIN(초안 SQLCODE 분류·EXPLAIN **검증**). 출처 = 표마다 URL · 직접 확인하지 못한 것은 **(추정)** · 각 절 끝 "추정 항목". 설계 결론은 D-359와 소스가 SSOT이고 이 문서는 고치지 않는다. 같은 작업에 섞여 있던 MS Access 방언 = **D-360**(사용자 요청 범위 밖 · 유지/제거 = 사용자 결정).

> ★ **10-25 방향 변경(사용자 "Nexa SQL과 드라이버 파일만 있으면 실행 가능해야" · D-361 후보)** = Db2 = **네이티브 DRDA 확장 드라이버 `nsql-driver-db2`**(순수 Rust · 외부 crate 0 · Oracle Thin과 같은 길 · 방언·SYSCAT·오류표 재사용) 예정 · 아래 ODBC 경로 = **Windows 폴백** · 조사 = [121b](121b-db2-drda-research.md).

## 결론(5줄) — 반영 = 커밋 4(a8384f65 방언·접속·Caps·오류·문법 · 194c7a8f SYSCAT(주의 8 반영) · bc97dc74 앱 접점 · c154d8f5 통합 시험 + `db2.yml`) + b79c1239(odbcinst 패키지)

1. **CI = 가능 · 별도 잡 권장** — `icr.io/db2_community/db2`(11.5.9.0 ≈1.6 GB · 12.1.x ≈2.6 GB · amd64만) · GitHub Actions `services:` + `--privileged`(또는 `--cap-add IPC_LOCK --cap-add IPC_OWNER`) · 준비 = 로그 `Setup has completed.` grep(시한 10분) · 공개 실측 잡 전체 ≈2분 남짓(11.5.5.1) → 본 integration 잡이 아니라 **`db2.yml`(수동 + 야간 + 경로 필터 PR)**.
2. **clidriver = 사용자/첫 사용 시 받기** — v12.1.4(2026-07-22) · 익명 HTTP(public.dhe.ibm.com · 로그인·클릭 동의 없음) · linuxx64 25 MB(glibc 2.34+ · x86_64-v2) · macarm64 18 MB(12.1부터) · macos64 21 MB(Intel = 11.5.9까지) · ntx64 26 MB · 재배포 조건 원문 미확인 → 설치본 동봉보다 사용자 설치/첫 사용 시 받기(node-ibm_db 방식)가 안전 · LUW 접속 = 무료(z/OS·IBM i = db2connect 라이선스).
3. **카탈로그 초안 = 열·코드 전부 문서와 일치(✗ 0) · 주의 8** — VALID 'X' 표시 누락 · 트리거 TRIGEVENT 'M' · TABCONST 'I'(함수 종속)를 CHECK로 · FK reftabschema 누락 · 모듈 루틴 섞임 · 인덱스 INCLUDE 열 · STRINGUNITSLENGTH · "DEFAULT" CLOB · 그 밖 = 객체 검색 N·G·U·W 누락 · 스키마 목록이 TABLES 기준이라 루틴만 있는 스키마 숨음 · `ENV_INST_INFO`는 권한 막힌 DB에서 실패 → 대체 경로.
4. **오류 분류 초안 = ✗ 2 · 주의 6** — `-668`(테이블 상태 · REORG/SET INTEGRITY 보류) ≠ Lock · `-613`(키 길이 한도) ≠ DataType · `-204` = 모든 객체 → NoObject · `-1013` = Connection · `-444` = 서버 설치 문제 · `-911` = 이유 코드 2/68로 분리 + 롤백 안내 · 오류 위치 = Db2가 주지 않음(SQL0104N 토큰 원문 검색뿐).
5. **특이 문법·EXPLAIN** — `EXPLAIN PLAN SELECTION FOR` ✓(= ALL) · 조회 = 표 이름 SYSTOOLS 한정 · REQUESTER+QUERYTAG로 우리 행 고정 · 트리 = EXPLAIN_STREAM 재귀 · `SYSINSTALLOBJECTS('EXPLAIN','C',NULL,NULL)`(빈 문자열 아님 · 기본 SYSTOOLS) · `OFFSET … FETCH FIRST` 11.1+(10.5 이하 = ROW_NUMBER) · `SAVEPOINT x ON ROLLBACK RETAIN CURSORS`(UNIQUE 빼기) · `SYSIBM.SYSDUMMY1` ✓ · 취소 = `SQLCancel` 1순위 → `WLM_CANCEL_ACTIVITY` → `FORCE APPLICATION`(권한) · 그리드 행 식별 = DBeaver는 `RID_BIT()`.

---

조사일 2026-10-25 · 웹 조사만 · 직접 확인하지 못한 것은 "추정"으로 표시

## ① CI용 Db2 Community 컨테이너

**결론**: 이미지는 `icr.io/db2_community/db2:12.1.x.0`(amd64)를 쓴다. ubuntu-latest의 `services:`에서 `options: --privileged`(또는 `--cap-add IPC_LOCK --cap-add IPC_OWNER`)로 실제로 돌아가는 공개 사례가 여럿 있다. 준비 판정은 로그의 `Setup has completed.` 한 줄로 한다. 실측은 컨테이너 생성 27~42초 + 대기 65~70초로 2분 남짓이다(11.5.5.1). 다만 이미지가 압축 기준 1.6~2.6 GB라서 본 integration 잡에 넣지 말고 **별도 `db2.yml`(수동 + 야간 + 경로 필터 PR)**로 두기를 권한다.

### 이미지·태그
| 항목 | 내용 | 출처 |
|---|---|---|
| 현행 레지스트리 | `icr.io/db2_community/db2` — 태그 `11.5.5.1` `11.5.8.0` `11.5.9.0` `12.1.0.0`~`12.1.5.0` `latest`(레지스트리 API `/v2/.../tags/list`로 직접 조회) | https://icr.io/v2/db2_community/db2/tags/list |
| 아키텍처 | manifest list = linux/amd64 · ppc64le · s390x (**arm64 없음** → Apple Silicon 로컬은 에뮬레이션) | 같은 레지스트리 manifest 조회 |
| 크기(amd64 · 압축 레이어 합) | 11.5.9.0 ≈ **1.58 GB** · 12.1.5.0 ≈ **2.63 GB** | 같은 레지스트리 manifest 조회 |
| 옛 이미지 | `ibmcom/db2`(Docker Hub · 최종 11.5.8.0 = latest) — "이 위치는 종료 예정, 11.5.9.0부터 icr.io" | https://hub.docker.com/r/ibmcom/db2 |
| 사용 한도 | Community Edition = 메모리 16 GB · 4코어로 자동 제한(최소 사양 명시 없음) | https://www.ibm.com/docs/en/db2/12.1.x?topic=system-linux |

### 환경 변수 (IBM 공식 `.env_list` 예시)
| 변수 | CI 권장값 | 의미 | 출처 |
|---|---|---|---|
| `LICENSE` | `accept` | 필수(약관 동의) | IBM docs 12.1 Linux · Docker Hub |
| `DB2INSTANCE` | `db2inst1` | 인스턴스 이름 = 접속 사용자 | 같음 |
| `DB2INST1_PASSWORD` | 임의 | 없으면 난수 생성 | Docker Hub |
| `DBNAME` | `testdb` | 초기 DB 생성(없으면 DB 없음) | 같음 |
| `BLU` · `ENABLE_ORACLE_COMPATIBILITY` | `false` | 컬럼 저장 · Oracle 호환 | IBM docs 12.1 |
| `UPDATEAVAIL` · `TO_CREATE_SAMPLEDB` · `REPODB` · `IS_OSXFS` · `HADR_ENABLED` | `NO` · `false` ×4 | 업데이트 · 샘플 DB · 저장소 DB · macOS FS · HADR(켜면 `ETCD_*` 필요) | IBM docs 12.1 · https://github.com/aurany/dbt-ibmdb2/blob/main/.github/workflows/test.yml |
| `PERSISTENT_HOME` | `false`(CI · 볼륨 없음) | 기본 true · `/database` 볼륨 전제 | Docker Hub · dbt-ibmdb2 |
| `ARCHIVE_LOGS` · `AUTOCONFIG` | `false` | **기동 시간 단축**(Docker Hub 명시 · Testcontainers도 옵션 제공) | Docker Hub · https://github.com/testcontainers/testcontainers-java/blob/main/modules/db2/src/main/java/org/testcontainers/db2/Db2Container.java |
| `SAMPLEDB` · `TEXT_SEARCH` | 생략 | 샘플 DB · 텍스트 검색 | Docker Hub |

### 권한·기동·준비 판정
| 항목 | 내용 | 출처 |
|---|---|---|
| privileged | IBM 문서 예시는 전부 `--privileged=true`. 대안은 `--cap-add IPC_LOCK --cap-add IPC_OWNER`(Testcontainers-java 기본 · dbt-ibmdb2 CI) | IBM docs 12.1 · Db2Container.java · https://community.ibm.com/community/user/question/icriodb2-communitydb211590-is-privileged-mode-always-required |
| `services:` 지원 | **된다**: `options: --privileged`(datajudge · slick) 또는 `--cap-add …`(dbt-ibmdb2). `docker run` 단계로 띄우는 예(loopback-ibmdb · pydrda)도 있다 | 아래 사례 표 |
| 포트 | 50000 | Docker Hub · IBM docs |
| 준비 로그 | `(*) Setup has completed.`(Testcontainers 정규식 `.*Setup has completed\..*` · 시한 10분) | Db2Container.java · Docker Hub |
| 대안 판정 | 마커 파일 `/database/config/.shared-data/setup_complete`(slick health-cmd). **`db2 connect` 탐침은 db2bp 고아 연결을 남겨 뒤의 `admin_drop_schema`에서 SQL0478N이 난다**는 경고가 있다 | https://github.com/slick/slick/blob/main/.github/workflows/ci.yml |
| 실측 기동 | datajudge 2026-05-12 run(ibmcom/db2:11.5.5.1 · `UPDATEAVAIL=NO`): Initialize containers **27~42초**(pull 포함) + "Wait for db2" **65~70초** → 잡 전체 2분 2초~2분 18초 | https://github.com/Quantco/datajudge/actions/runs/25744282374 |
| 12.1 함정 | 12.1.2에서 **재시작하면 인스턴스 구성 실패**(fencedid 권한) 보고 — CI처럼 매번 새로 띄우면 영향 없음 | https://community.ibm.com/community/user/discussion/ibm-db2-12-12-in-docker-cannot-be-restarted |

### 공개 GitHub Actions 사례
| 저장소 | 방식 | 대기 | 링크 |
|---|---|---|---|
| Quantco/datajudge | `services:` `ibmcom/db2:11.5.5.1` · `options: --privileged` · **PR 라벨 `db2`/`ready` 또는 main일 때만** 도는 별도 잡 | `until docker logs "${{ job.services.db.id }}" \| grep -q 'Setup has completed.'; do sleep 5; done`(timeout 2분) | https://github.com/Quantco/datajudge/blob/main/.github/workflows/ci.yaml |
| aurany/dbt-ibmdb2 | `services:` `ibmcom/db2:11.5.7.0` · `--cap-add IPC_LOCK/IPC_OWNER` · env 전체 나열 | `docker logs -f <id> \| grep -m 1 '(*) Setup has completed.'` | https://github.com/aurany/dbt-ibmdb2/blob/main/.github/workflows/test.yml |
| slick/slick | `services:` `ibmcom/db2:11.5.8.0` · `--privileged` + `--health-cmd` 마커 파일(start-period 90s · retries 60). 같은 시각에 함께 뜨는 SQL Server가 죽어서 그쪽은 뒤 단계로 뺐다 | 서비스 health | https://github.com/slick/slick/blob/main/.github/workflows/ci.yml |
| loopbackio/loopback-ibmdb | `docker run -itd --privileged=true` 단계 | `docker exec db2 su - db2inst1 -c "db2 connect to …"` 5초×20 | https://github.com/loopbackio/loopback-ibmdb/blob/master/.github/workflows/continuous-integration.yml |
| nakagami/pydrda | `docker run --privileged=true` · 별도 `test_db2.yml` | 드라이버 접속 성공 3회 · timeout 12분 | https://github.com/nakagami/pydrda/blob/master/.github/workflows/test_db2.yml |

### 권고 — 별도 `db2.yml`
- **이유**: 이미지 pull이 1.6~2.6 GB, 기동이 1~2분(12.1은 더 길 수 있음 · 추정)이고 privileged 컨테이너가 같은 러너의 다른 서비스와 CPU를 다툰다(slick 사례). 본 integration 잡(PG·Oracle·MSSQL)에 넣으면 PR마다 3~5분이 늘고 불안정 요인이 섞인다.
- **형태**: `on: workflow_dispatch` + `schedule`(야간) + `pull_request` `paths:`(`crates/nsql-driver-db2*/**` 등) · `services:` `icr.io/db2_community/db2:12.1.x.0` 버전 고정 · `options: --privileged` · env에 `ARCHIVE_LOGS=false` `AUTOCONFIG=false` `UPDATEAVAIL=NO` `PERSISTENT_HOME=false` · 대기 = datajudge식 로그 grep에 `timeout-minutes: 10`(Testcontainers와 같은 상한) · 실패하면 `docker logs` 덤프 · clidriver tar.gz는 `actions/cache`(키 = 파일명 + Last-Modified).
- 탐침은 CLP `db2 connect`가 아니라 로그 grep이나 마커 파일로 한다(slick의 SQL0478N 경고).
- ★ **다른 판 시험(10-31 · 6253fab7)** = `gh workflow run db2.yml -f image=icr.io/db2_community/db2`(태그 생략 = latest · 다른 태그도 같은 입력) · 입력 없는 실행(야간·PR) = 고정 `12.1.2.0` · 이 PC = Docker/podman/WSL 없음 → 로컬 컨테이너 시험 불가 · 코드스페이스(10-31 삭제) 대신 이 길 · ⚠ 10.5는 공개 이미지가 없어 이 입력으로도 불가(10-29 확인).

## ② IBM clidriver

**결론**: 최신은 **v12.1.4**(2026-07-22)다. 공용 HTTP 디렉터리에서 로그인·클릭 동의 없이 받는다(linuxx64 25 MB · macarm64 18 MB · ntx64 26 MB). IBM은 "조건부로 애플리케이션 설치본에 넣어 royalty-free 재배포 가능"이라고 밝히지만 조건 전문(재배포 허용 파일 목록·고지 유지·EULA 보호 수준)을 지켜야 하므로, 설치본 동봉보다 **ibm_db식 첫 사용 시 다운로드**(D-350 자동 설치와 같은 모양)가 안전하다. macOS arm64는 v12.1.0부터 공식 지원하고, Intel 맥은 v11.5.9만 지원한다.

### 다운로드 (https://public.dhe.ibm.com/ibmdl/export/pub/software/data/db2/drivers/odbc_cli/)
| 파일 | 대상 | 크기 · 갱신 | 비고 |
|---|---|---|---|
| `linuxx64_odbc_cli.tar.gz` | Linux x86-64 | 25 MB · 2026-07-22 | v12.1 = **glibc 2.34+ · x86_64-v2 CPU** 필요(구형은 v11.5.9) — python-ibmdb README |
| `macarm64_odbc_cli.tar.gz` | macOS arm64 | 18 MB · 2026-07-22 | v12.1.0부터 |
| `macos64_odbc_cli.tar.gz` | macOS x64 | 21 MB · 2026-01-20 | v11.5.x만(기본 v11.5.9) |
| `ntx64_odbc_cli.zip` | Windows x64 | 26 MB · 2026-07-22 | |
| 버전별 폴더 | `v11.5.9/` `v12.1.0/` `v12.1.2/` `v12.1.4/`(최신 2026-07-22) 등 | | 루트 파일 = 최신본 · 고정하려면 버전 폴더를 쓴다 |

### 라이선스·재배포
| 항목 | 내용 | 출처 |
|---|---|---|
| 재배포 | "설치 패키지에 드라이버를 포함할 수 있다 · **특정 조건 하에서** royalty-free 재배포 가능" · ISV 배포용으로 설계 | https://www.ibm.com/support/pages/node/398303 |
| 조건(License Information) | 목적 코드로만 · 라이선시의 부가가치 애플리케이션 안에서만 · 저작권 고지 삭제 금지 · IBM 상표로 홍보 금지 · 최종 사용자 EULA가 IBM 조건 이상으로 IBM을 보호할 것 · 무보증. **IBM 원문이 아니라 제3자 미러의 번역본에서 확인했다** | https://apps.meisa-e.com/bbdd_sync/Servicio/license/Windows/odbc_LI_fr.rtf |
| 다운로드 동의 | public.dhe.ibm.com은 익명 HTTP 디렉터리라 클릭 동의가 없다. 아카이브 안의 `license/` 폴더에 LI 문서가 있다(추정) | 디렉터리 목록 |
| 접속 대상 라이선스 | Db2 LUW·Informix = 무료 · **Db2 for z/OS·IBM i = db2connect 라이선스 필요**(없으면 SQL1598N · `db2con*.lic`를 `clidriver/license`에 · 버전이 드라이버와 일치해야 함) | https://github.com/ibmdb/node-ibm_db/blob/master/README.md · python-ibmdb README |

### ibm_db 패키지가 받는 방식
| 패키지 | 방식 | 출처 |
|---|---|---|
| node-ibm_db | `npm install` 때 public 저장소에서 플랫폼별 tar.gz를 받는다 · `IBM_DB_HOME`(기존 설치) 또는 `IBM_DB_INSTALLER_URL`(로컬 아카이브)이면 받지 않는다 · 버전 = `-clidriver=` / `CLIDRIVER_DOWNLOAD_VERSION` · `DOWNLOAD_CLIDRIVER=true`면 강제 · 3.3.0부터 기본 v12.1 | https://github.com/ibmdb/node-ibm_db/blob/master/README.md |
| python ibm_db | **wheel에 clidriver 동봉**(site-packages/clidriver · 위 env 무시) · 소스 설치 때만 다운로드(`CLIDRIVER_VERSION`) · arm64 맥 = v12.1.0 · Intel 맥 = v11.5.9 | https://github.com/ibmdb/python-ibmdb/blob/master/README.md |
| 함의 | python 패키지 자체가 clidriver를 동봉해 재배포하는 선례다(IBM 자사 프로젝트). 제3자 동봉의 허용 범위는 위 LI 조건으로 판단한다 | — |

### 등록·환경 변수
| 항목 | 내용 | 출처 |
|---|---|---|
| unixODBC | `odbcinst.ini`에 `[Db2]` 절 + `Driver = <설치경로>/lib/libdb2o.so`(위치 = `odbcinst -j`) · ★ **10-25 CI 실측 정정 = v12.1.4 linuxx64에는 `libdb2o.so`가 없고 `libdb2.so`/`libdb2.so.1`뿐** → `Driver = …/lib/libdb2.so` | https://www.ibm.com/support/knowledgecenter/SSEPGG_11.1.0/com.ibm.db2.luw.apdv.cli.doc/doc/t0061216.html |
| macOS | `lib/libdb2o.dylib`(추정) · 로드 실패 시 `DYLD_LIBRARY_PATH=<clidriver>/lib`, SQL1042C면 `lib/icc` 추가 | python-ibmdb README |
| Windows | `db2cli install -setup`(관리자) = 레지스트리 ODBC 등록 + cfg·cfgcache·db2dump 생성 · `-cleanup`으로 해제 · `-copyname` · `registerdsn` | https://www.ibm.com/docs/en/db2/11.5.x?topic=commands-db2cli-db2-interactive-cli |
| Windows 드라이버 이름 | `IBM DB2 ODBC DRIVER`(+ ` - <copyname>`) — 문서에서 확인하지 못함(추정) · `db2oreg1 -i`는 옛 등록 도구(추정) | — |
| 라이브러리 경로 | Linux `LD_LIBRARY_PATH` · macOS `DYLD_LIBRARY_PATH` · Windows `PATH`(SQL1042C면 `bin\amd64.VC12.CRT`) · AIX `LIBPATH`. IBM은 **PATH류 대신 설치 폴더에서 동적 로드를 권장**한다(여러 사본이 섞이는 것 방지) → 우리 cdylib의 지연 dlopen과 맞는다 | https://www.ibm.com/docs/en/SSEPGG_12.1.0/com.ibm.db2.luw.apdv.cli.doc/doc/t0023864.html · python-ibmdb README |
| `DB2_CLI_DRIVER_INSTALL_PATH` | CLI 드라이버 위치를 지정하는 변수. SAP 노트 미리보기에서만 확인했고 IBM 원문은 찾지 못함(추정) | https://userapps.support.sap.com/sap/support/knowledge/en/2725498 |
| `IBM_DB_HOME` | ibm_db 계열의 규약일 뿐 드라이버 자체 변수는 아니다 | 두 README |
| 설정 파일 | `cfg/db2dsdriver.cfg`(XML · DSN/DB별 매개변수·DSN 별칭) · `db2cli.ini`(사본마다 하나) · `db2cli validate`로 둘을 점검 · `registerdsn`은 둘 중 한 곳에 DSN이 있어야 함. 연결 문자열만으로 접속하면 둘 다 없어도 된다(추정) | db2cli 문서 · https://www.ibm.com/docs/SSEPGG_11.5.0/com.ibm.db2.luw.apdv.cli.doc/doc/t0024166.html |

### 연결 문자열 키워드 (SQLDriverConnect · 전부 IBM 키워드 목록에 있음)
| 키워드 | 분류(IBM) | 예 |
|---|---|---|
| `DRIVER` | ODBC 표준 | `{Db2}`(odbcinst 절 이름) · Windows = 위 드라이버 이름 |
| `DATABASE` `HOSTNAME` `PORT` `PROTOCOL` | File DSN | `DATABASE=testdb;HOSTNAME=localhost;PORT=50000;PROTOCOL=TCPIP` |
| `UID` `PWD` | Data source | |
| `CurrentSchema` | Enterprise | 기본 스키마 |
| `Security` `SSLServerCertificate` `SSLClientKeystoredb` | File DSN | `Security=SSL;SSLServerCertificate=/path/ca.arm` |
| `Authentication` | File DSN | `SERVER_ENCRYPT` 등(값 목록은 추정) |
| `ConnectTimeout` `ReceiveTimeout` `QueryTimeoutInterval` | Environment | 질의 시한 자체는 `SQL_ATTR_QUERY_TIMEOUT` 속성으로 걸고, `QueryTimeoutInterval`은 확인 간격(추정) |
| `AutoCommit` | Transaction | |

출처 = https://www.ibm.com/docs/SSEPGG_11.5.0/com.ibm.db2.luw.apdv.cli.doc/doc/r0007964.html · 기본 문자열 예 = node-ibm_db README

## 추정 항목
- 12.1.x 이미지의 기동 시간이 11.5보다 길다는 것(실측은 11.5.5.1만 확인 · 1~2분 · IBM 가이드는 "최대 5분").
- `ARCHIVE_LOGS=false`·`AUTOCONFIG=false`가 icr.io 12.1 이미지에서도 똑같이 먹는지(Docker Hub의 ibmcom 설명과 Testcontainers 근거).
- `--cap-add IPC_LOCK/IPC_OWNER`만으로 12.1이 뜨는지(dbt-ibmdb2는 11.5.7 사례) — 처음에는 `--privileged`로 시작하기를 권한다.
- Windows ODBC 드라이버 등록 이름 `IBM DB2 ODBC DRIVER`(- copyname) · `db2oreg1` 용도 · macOS 라이브러리 파일 이름 `libdb2o.dylib`.
- `DB2_CLI_DRIVER_INSTALL_PATH`의 정확한 동작(IBM 원문 미확인).
- 재배포 조건 전문: IBM 원문이 아니라 제3자 미러의 번역본으로 확인 → 실제 동봉을 결정하기 전에 v12.1.4 아카이브 안의 `license/` LI 원문을 직접 읽어야 한다.
- 아카이브에 LI 문서가 들어 있다는 것, 연결 문자열만 쓰면 db2dsdriver.cfg가 필요 없다는 것, `Authentication` 값 목록과 `QueryTimeoutInterval`의 의미.

---

조사일 2026-10-25 · 원천 = IBM Db2 11.5 문서(SYSCAT 뷰 · 명령 · 루틴) + DBeaver `master` 얕은 복제(`plugins/org.jkiss.dbeaver.ext.db2`) 직접 읽음.
DBeaver 경로 기준 = `https://github.com/dbeaver/dbeaver/blob/devel/plugins/org.jkiss.dbeaver.ext.db2/src/org/jkiss/dbeaver/ext/db2/` (아래 `…/` 로 줄임).

## ③ 카탈로그(SYSCAT)

**결론**: 탐색기·메타데이터는 전부 `SYSCAT.*` 뷰를 **스키마 단위 한 번 + `WITH UR`**(잠금 없이 읽기)로 읽으면 된다. PUBLIC이 SELECT를 가진다(제한 DB가 아니면). 소스 = `ROUTINES.TEXT`·`VIEWS.TEXT`·`TRIGGERS.TEXT` 그대로(CLOB), 테이블 DDL은 카탈로그로 직접 조립하는 게 권장(DB2LK는 비문서·부작용).
취소 = **같은 연결의 CLI `SQLCancel`이 1순위**, 서버 측은 `WLM_CANCEL_ACTIVITY`(연결 유지) > `FORCE APPLICATION`(연결 끊김 · SYSADM/SYSCTRL/SYSMAINT).

### 객체 종류 코드

| 뷰.열 | 코드 |
|---|---|
| `TABLES.TYPE` | A 별칭 · G 생성 임시 테이블 · H 계층 테이블 · L 분리(detached) 테이블 · N 닉네임 · S MQT · T 테이블 · U **타입드 테이블** · V 뷰 · W 타입드 뷰 (IBM 문서 확인) |
| `TABCONST.TYPE` | P 기본키 · U 유일 · F 외래키 · K 체크 · I 함수 종속(정보 제약) · `ENFORCED` Y/N |
| `INDEXES.UNIQUERULE` | D 중복 허용 · U 유일 · P 기본키 구현 인덱스 |
| `ROUTINES.ROUTINETYPE` | P 프로시저 · F 함수 · M 메서드 · 오버로드 구분 = `SPECIFICNAME` |
| `ROUTINEPARMS.ROWTYPE` | P 입력 · O 출력 · B 입출력 · R 결과(캐스트 전) · C 결과(캐스트 후) · S 집계 상태 · `ORDINAL` 1부터(그 밖 0) |
| `COLUMNS` | `NULLS` Y/N · `IDENTITY` Y/N · `GENERATED` A(ALWAYS)/D(BY DEFAULT)/공백 · `DEFAULT`(식 문자열) · `KEYSEQ`(PK 순번) · `HIDDEN` · `COLNO` 0부터 |

### 권장 SQL (모두 끝에 `WITH UR` 권장 — DBeaver 관례)

| 대상 | 권장 SQL | 비고 |
|---|---|---|
| 스키마 | `SELECT SCHEMANAME, OWNER, REMARKS FROM SYSCAT.SCHEMATA ORDER BY SCHEMANAME` | 시스템 스키마 숨김(아래) |
| 테이블류 | `SELECT TABNAME, TYPE, CARD, REMARKS, BASE_TABSCHEMA, BASE_TABNAME, STATUS FROM SYSCAT.TABLES WHERE TABSCHEMA=? ORDER BY TABNAME` | TYPE으로 폴더 분배 · 별칭 대상 = BASE_* · `STATUS` C(검사 대기)·X(비작동) |
| 컬럼 | `SELECT COLNAME, COLNO, TYPESCHEMA, TYPENAME, LENGTH, SCALE, NULLS, DEFAULT, IDENTITY, GENERATED, TEXT, KEYSEQ, HIDDEN, REMARKS FROM SYSCAT.COLUMNS WHERE TABSCHEMA=? AND TABNAME=? ORDER BY COLNO` | `TEXT` = 생성 열 식 · 스키마 전체는 `ORDER BY TABNAME, COLNO`(지연 로딩이면 테이블 단위) |
| PK/UK | `SELECT C.CONSTNAME, C.TYPE, C.ENFORCED, K.COLNAME, K.COLSEQ FROM SYSCAT.TABCONST C JOIN SYSCAT.KEYCOLUSE K ON K.TABSCHEMA=C.TABSCHEMA AND K.TABNAME=C.TABNAME AND K.CONSTNAME=C.CONSTNAME WHERE C.TABSCHEMA=? AND C.TABNAME=? AND C.TYPE IN ('P','U') ORDER BY C.CONSTNAME, K.COLSEQ` | DBeaver와 동일 |
| FK | `SELECT R.CONSTNAME, R.REFTABSCHEMA, R.REFTABNAME, R.REFKEYNAME, R.DELETERULE, R.UPDATERULE, K.COLNAME, K.COLSEQ FROM SYSCAT.REFERENCES R JOIN SYSCAT.KEYCOLUSE K ON … WHERE R.TABSCHEMA=? AND R.TABNAME=? ORDER BY R.CONSTNAME, K.COLSEQ` | 참조 쪽 컬럼 = REFKEYNAME의 KEYCOLUSE · 역참조 = `WHERE R.REFTABSCHEMA=? AND R.REFTABNAME=?` |
| CHECK | `SELECT C.CONSTNAME, C.TEXT, CK.COLNAME FROM SYSCAT.CHECKS C LEFT JOIN SYSCAT.COLCHECKS CK ON … WHERE C.TABSCHEMA=? AND C.TABNAME=?` | `CHECKS.TYPE` C(체크)·A/F/R 등 시스템 생성도 섞임(추정 — 걸러낼 것) |
| 인덱스 | `SELECT I.INDNAME, I.INDSCHEMA, I.UNIQUERULE, I.INDEXTYPE, U.COLNAME, U.COLSEQ, U.COLORDER FROM SYSCAT.INDEXES I JOIN SYSCAT.INDEXCOLUSE U ON U.INDSCHEMA=I.INDSCHEMA AND U.INDNAME=I.INDNAME WHERE I.TABSCHEMA=? AND I.TABNAME=? ORDER BY I.INDNAME, U.COLSEQ` | `COLORDER` A/D/I(INCLUDE) · **인덱스 스키마 ≠ 테이블 스키마 가능** |
| 루틴 | `SELECT ROUTINENAME, SPECIFICNAME, ROUTINETYPE, LANGUAGE, ORIGIN, PARM_COUNT, REMARKS FROM SYSCAT.ROUTINES WHERE ROUTINESCHEMA=? AND ROUTINEMODULENAME IS NULL ORDER BY ROUTINENAME` | 소스 = 같은 행 `TEXT`(지연) · 외부(C/Java) 루틴은 TEXT NULL |
| 매개변수 | `SELECT PARMNAME, ROWTYPE, ORDINAL, TYPENAME, LENGTH, SCALE FROM SYSCAT.ROUTINEPARMS WHERE ROUTINESCHEMA=? AND SPECIFICNAME=? ORDER BY ROWTYPE, ORDINAL` | 시그니처 키 = SPECIFICNAME |
| 뷰 소스 | `SELECT TEXT FROM SYSCAT.VIEWS WHERE VIEWSCHEMA=? AND VIEWNAME=?` | `CREATE VIEW …` 전문 · MQT도 VIEWS에 있음 |
| 트리거 | `SELECT TRIGNAME, TABSCHEMA, TABNAME, TRIGTIME, TRIGEVENT, GRANULARITY, VALID, TEXT, REMARKS FROM SYSCAT.TRIGGERS WHERE TRIGSCHEMA=?` | TEXT = `CREATE TRIGGER …` 전문 |
| 시퀀스 | `SELECT SEQNAME, SEQTYPE, START, INCREMENT, MINVALUE, MAXVALUE, CYCLE, CACHE, ORDER, REMARKS FROM SYSCAT.SEQUENCES WHERE SEQSCHEMA=? AND SEQTYPE='S'` | SEQTYPE A 별칭 · I 식별(IDENTITY) 시퀀스 · S 일반(IBM 문서 확인) |
| 주석 | 각 뷰의 `REMARKS` 열(TABLES·COLUMNS·ROUTINES·INDEXES·TRIGGERS·SEQUENCES·SCHEMATA·TABCONST) | 쓰기 = `COMMENT ON TABLE/COLUMN …` |
| 현재 스키마 | `VALUES CURRENT SCHEMA` · 변경 `SET CURRENT SCHEMA = ?` | DBeaver `DB2ExecutionContext` 동일 |
| 행 수 추정 | `SYSCAT.TABLES.CARD` | **통계 없음 = -1**(IBM 문서) → "?"로 표시 · 정확값은 `COUNT(*)` 별도 |
| 크기 | `SELECT TABNAME, SUM(DATA_OBJECT_P_SIZE+INDEX_OBJECT_P_SIZE+LONG_OBJECT_P_SIZE+LOB_OBJECT_P_SIZE+XML_OBJECT_P_SIZE) FROM TABLE(SYSPROC.ADMIN_GET_TAB_INFO(?,'')) GROUP BY TABNAME` | KB 단위 · 무거움 → 요청 시만 |

### 시스템 스키마(숨김 목록)
DBeaver `DB2Schema.SYSTEM_SCHEMA` = `SYS, SYSCAT, SYSFUN, SYSIBM, SYSIBMADM, SYSIBMINTERNAL, SYSIBMTS, SYSPROC, SYSPUBLIC, SYSSTAT, SYSTOOLS, DB2QP, SQLJ, NULLID`. 권장 = 이 목록 + `SYS%` 접두 규칙(추정 — IBM은 `SYS` 접두를 예약).

### 세션·취소·DDL

| 항목 | 권장 | 권한 |
|---|---|---|
| 내 세션 ID | `VALUES (MON_GET_APPLICATION_HANDLE(), APPLICATION_ID())` | 없음(스칼라) |
| 세션 목록 | `SELECT APPLICATION_HANDLE, APPLICATION_NAME, SESSION_AUTH_ID, CLIENT_HOSTNAME FROM TABLE(MON_GET_CONNECTION(NULL,-2))` | EXECUTE on 루틴 또는 DATAACCESS/DBADM/SQLADM(추정 — 모니터 함수 공통 규칙) · DBeaver는 `SYSIBMADM.APPLICATIONS` |
| 실행 중 문장 ID | `SELECT UOW_ID, ACTIVITY_ID FROM TABLE(MON_GET_ACTIVITY(?, -2))` | 위와 같음(추정) |
| 문장만 취소(서버 측) | `CALL SYSPROC.WLM_CANCEL_ACTIVITY(handle, uow_id, activity_id)` → 대상은 SQL4725N(57014) · **연결 유지** | EXECUTE · DATAACCESS · DBADM · SQLADM · WLMADM 중 하나(PUBLIC 기본 없음) |
| 연결 강제 종료 | `CALL SYSPROC.ADMIN_CMD('FORCE APPLICATION (handle)')` · 비동기(MODE ASYNC만) · 연결 끊김 | ADMIN_CMD EXECUTE(+ 명령 자체 SYSADM/SYSCTRL/SYSMAINT — ADMIN_CMD 경유 시에도 필요한지는 추정) |
| 클라이언트 취소 | Db2 CLI/ODBC `SQLCancel`(다른 스레드에서 실행 중 문장 중단) — JDBC `Statement.cancel`과 같은 계열 | 권한 불필요 · **1순위**(DR-34 ■ 버튼) |
| 테이블 DDL | `CALL SYSPROC.DB2LK_GENERATE_DDL('-e -x -xd -td ; -t SCHEMA.TAB', ?)` → `SELECT SQL_STMT FROM SYSTOOLS.DB2LOOK_INFO WHERE OP_TOKEN=? ORDER BY OP_SEQUENCE` → `CALL SYSPROC.DB2LK_CLEAN_TABLE(?)` | **비문서 SP**(DBeaver 주석 "Undocumented") · SYSTOOLS 테이블 생성 권한 · 이름 공백 불가 · 시스템 테이블 불가 → **카탈로그 조립이 기본, DB2LK는 선택** |

출처(IBM): [SYSCAT.TABLES](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscattables) · [SYSCAT.TABCONST](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscattabconst) · [SYSCAT.ROUTINEPARMS](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscatroutineparms) · [FORCE APPLICATION](https://www.ibm.com/docs/en/db2/11.5.x?topic=commands-force-application) · [ADMIN_CMD](https://ibm.com/docs/SSEPGG_11.5.0/com.ibm.db2.luw.sql.rtn.doc/doc/r0012547.html) · [ADMIN_CMD FORCE APPLICATION](https://www.ibm.com/docs/en/SS6NHC/com.ibm.db2.luw.sql.rtn.doc/doc/r0023574.html) · [WLM_CANCEL_ACTIVITY](https://www.ibm.com/support/knowledgecenter/SSEPGG_11.1.0/com.ibm.db2.luw.sql.rtn.doc/doc/r0052912.html)

## ③-검증 (초안 `db2-catalog-sql.txt` 대조)

**결론**: 열 이름·코드 값은 **전부 IBM 11.5 SYSCAT 정의와 일치(✗ 0)** — `TYPE` T/V/S/A·N, `STATUS` X/C, `VALID`, `UNIQUERULE` P/U, `SEQTYPE` S, `REFERENCES` 조인 열, `ENV_INST_INFO.SERVICE_LEVEL`(VARCHAR(128))·`FIXPACK_NUM`(INTEGER) 모두 존재.
고칠 점은 **표시 정확도(주의 8건)**: `VALID='X'`(비작동) 누락 · 트리거 `TRIGEVENT='M'` · 제약 `TYPE='I'`를 CHECK로 오분류 · FK 대상 스키마 누락 · 모듈 루틴 혼입 · INCLUDE 열 · 문자 단위 길이 · `DEFAULT` CLOB.
**TRIM**: SYSCAT 이름 열은 전부 `VARCHAR(128)`(CHAR 아님)이라 꼬리 공백 패딩이 없다 → TRIM은 불필요(무해). 단, 구분 이름(`"A "`)의 공백까지 지우므로 **되돌려 WHERE에 쓰는 키는 TRIM하지 않은 값**이 안전. 코드 열(`TYPE` 등)은 `CHAR(1)`이라 비교에 영향 없음.

| 질의(요약) | 판정 | 근거 | 고칠 점 |
|---|---|---|---|
| 객체 검색 `TABLES type IN ('T','V','S','A')` ∪ ROUTINES P/F ∪ TRIGGERS | 주의 | [TABLES](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscattables) | 코드 ✓(S=MQT·A=별칭). **N(닉네임)·G(생성 임시)·U(타입드 테이블)·W(타입드 뷰)·H** 빠짐 → 필요하면 추가. ROUTINES에 `routinemodulename IS NULL` 없으면 모듈 루틴이 섞임 |
| 컬럼 검색 `COLUMNS … 'TABLE' … ORDER BY colno` | ✓ | [COLUMNS](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscatcolumns) | 뷰 컬럼도 `'TABLE'`로 표기됨(표시만) |
| 본문 검색 `ROUTINES/VIEWS/TRIGGERS text LIKE` | 주의 | [LIKE](https://www.ibm.com/docs/en/db2/11.5.x?topic=predicates-like-predicate) · [ROUTINES](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscatroutines) | TEXT = `CLOB(2M)`. match 쪽 CLOB는 금지 조항 없음(추정 ✓), **pattern 쪽은 CLOB 불가 · 32 672바이트 이하** → 리터럴/VARCHAR 매개변수로. 외부 루틴은 TEXT NULL. VIEWS에는 MQT 포함(`'V'` 고정 표기 주의) |
| 탐색 합집합 (+ SEQUENCES `seqtype='S'` · INDEXES) | ✓ | [SEQUENCES](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscatsequences) | S/I/A 중 S만 = 맞음(I = IDENTITY 내부) |
| 스키마 `SCHEMATA` + `schemaname IN (SELECT DISTINCT tabschema FROM TABLES)` | 주의 | — | 루틴·시퀀스만 있는 스키마가 숨음 → 의도 확인. 시스템 = SYS*·NULLID·SQLJ ✓(DBeaver는 + `DB2QP`) |
| 버전 `SERVICE_LEVEL`·`FIXPACK_NUM` FROM `SYSIBMADM.ENV_INST_INFO` | ✓(주의) | [ENV_INST_INFO](https://www.ibm.com/docs/en/SSFMBX/com.ibm.swg.im.dashdb.sql.rtn.doc/doc/r0022040.html) | 권한 = SELECT/CONTROL 또는 DATAACCESS·DBADM·SQLADM·ACCESSCTRL·SECADM · **비제한 DB면 PUBLIC SELECT**. 제한(RESTRICTIVE) DB에서 실패 시 `CURRENT SERVER`만으로 폴백 필요. 11.5는 SERVICE_LEVEL에 이미 `v11.5.x.y`가 들어 있어 `FP`는 중복(모드팩 체계에서 FIXPACK_NUM이 0일 수 있음 · 추정) |
| 시스템 표 목록 `TABLES WHERE tabschema IN ('SYSCAT','SYSIBMADM','SYSSTAT')` | ✓ | TABLES | SYSCAT·SYSSTAT은 뷰(TYPE V) — 무방 |
| PK/UK `TABCONST ⨝ KEYCOLUSE · type IN ('P','U') ORDER BY colseq` | ✓ | [TABCONST](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscattabconst) | DBeaver와 동일 |
| 설명 `TABLES/COLUMNS.remarks` | ✓ | TABLES·COLUMNS | REMARKS = VARCHAR(254) NULL 허용 |
| 제약 전체 `CASE type P/U/F→R/ELSE C` ⨝ KEYCOLUSE ⨝ REFERENCES | 주의 | TABCONST · [REFERENCES](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscatreferences) | 조인 열(constname·tabschema·tabname) ✓. **`TYPE='I'`(함수 종속 · 정보 제약)이 'C'로 오분류** → `WHEN 'K' THEN 'C'`로 바꾸고 I는 제외/별도. FK 대상 = `reftabname`뿐 → `reftabschema` 추가(다른 스키마 참조). `ENFORCED='N'`(정보 제약) 표시 고려 |
| 인덱스 `INDEXES ⨝ INDEXCOLUSE · uniquerule IN ('P','U')` | 주의 | [INDEXES](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscatindexes) · [INDEXCOLUSE](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscatindexcoluse) | UNIQUERULE D/U/P ✓. **`COLORDER='I'`(INCLUDE) 열도 키처럼 나옴** → 구분 표시. XML·텍스트 인덱스(INDEXTYPE XPTH/XRGN/TEXT 등)도 섞임 |
| 종류별 목록 TABLES `status X/C` · `VARCHAR_FORMAT(alter_time …)` · 별칭 base_* | ✓ | TABLES | STATUS C/N/X ✓ · ALTER_TIME TIMESTAMP NOT NULL ✓ · BASE_* = TYPE 'A'일 때만 ✓ |
| 종류별 목록 ROUTINES `valid 'N'` · specificname | 주의 | ROUTINES | VALID = **N 재바인드 필요 · X 비작동 · Y** → `WHEN 'X' THEN 'INOPERATIVE'` 추가. 모듈 루틴 제외 조건 추가 |
| 종류별 목록 TRIGGERS `valid 'N'` · create_time | 주의 | [TRIGGERS](https://www.ibm.com/docs/en/db2/11.5.x?topic=views-syscattriggers) | VALID에 X(비작동) 추가 · CREATE_TIME ✓ |
| 종류별 목록 SEQUENCES `seqtype='S'` · alter_time | ✓ | SEQUENCES | ALTER_TIME·CREATE_TIME 둘 다 있음 |
| 종류별 목록 INDEXES `indschema` · create_time · uniquerule | ✓(주의) | INDEXES | 인덱스 스키마 ≠ 테이블 스키마 가능 → `tabschema.tabname`으로 표기 권장 |
| 컬럼 상세 `typename, length, length, scale, nulls, colno+1, "DEFAULT"` | 주의 | COLUMNS | COLNO 0부터 ✓ · `"DEFAULT"` 인용 ✓. **DEFAULT = CLOB(64K)** → `COALESCE(VARCHAR("DEFAULT", 254), '')`처럼 VARCHAR 변환 권장(드라이버 LOB 처리 회피). LENGTH: DECIMAL=정밀도 ✓ · **TIMESTAMP=10·DECFLOAT=바이트·유니코드 DB의 CODEUNITS32 열은 `STRINGUNITSLENGTH`가 선언 길이** → 문자열은 `COALESCE(stringunitslength, length)` |
| 소스 `VIEWS/ROUTINES/TRIGGERS text` · 오버로드 = `ORDER BY specificname` | ✓(주의) | VIEWS · ROUTINES · TRIGGERS | TEXT = "exactly as typed" CREATE 전문 → **스키마 한정이 없을 수 있음**(보수적 생성 원칙 · 100 §5) · VIEWS는 MQT 포함 |
| 인덱스 컬럼 `INDEXCOLUSE colorder 'D'→DESC` | 주의 | INDEXCOLUSE | A/D/**I(INCLUDE)/R(Random)** → I·R 표시 추가 |
| 참조됨 `REFERENCES WHERE reftabschema/reftabname` | ✓ | REFERENCES | 열 이름 ✓(DELETERULE A/C/N/R · UPDATERULE A/R 추가 표시 가능) |
| 테이블 트리거 `trigtime A/B/else` · `trigevent I/U/D/else` | 주의 | TRIGGERS | TRIGTIME A/B/I ✓. **TRIGEVENT 'M'(다중 이벤트)** → `EVENTINSERT/EVENTUPDATE/EVENTDELETE`(Y/N)로 조립 |

## ⑤ DBeaver Db2 LUW 대조

**결론**: DBeaver는 스키마마다 캐시 하나씩 `SELECT * FROM SYSCAT.<뷰> WHERE <스키마>=? … WITH UR`(+ 단건 조회판)로 읽는 단순 구조 — 우리 권장 SQL과 같은 뷰·조인이다(열은 `*` 대신 필요한 것만 고르면 됨).
그리드 편집 행 식별 = **PK/유일 키 우선, 없으면 의사 열 `RID_BIT()`**(v9.5+ · BigSQL 제외) — 우리 데이터 보호 불변식(1행 사전 검사)과 결합 가능.

### 탐색기 트리 (`plugin.xml` navigator)
Schemas › Schema › **Tables**(Columns · Unique Keys · Foreign Keys · References · Check Constraints · Indexes · Partitions · Periods · Triggers · Column Masks) · **Views**(Columns · Dependencies) · **MQTs** · **Indexes**(스키마 단위) · **Sequences** · **Aliases** · **Nicknames** · **Triggers** · **XML Schemas** · **Modules**(Functions · Procedures · Types · Variables · Conditions) · **Procedures**/**Functions**(UDF) · **UDTs** · **Packages**. DB 단위 = Data Types · Global Variables · Bufferpools · Tablespaces · Storage Groups · DB/DBM 파라미터 · Roles · 권한(Authorizations) · Federation(Wrappers · Remote Servers · User Mappings).

### 캐시별 SQL (파일 경로)

| 노드 | 파일 | SQL 요지 |
|---|---|---|
| 스키마 | `…/model/DB2DataSource.java` | `SELECT * FROM SYSCAT.SCHEMATA ORDER BY SCHEMANAME WITH UR` |
| 테이블 | `…/model/cache/DB2TableCache.java` | `SYSCAT.TABLES WHERE TABSCHEMA=? AND TYPE IN ('G','H','L','T','U')` · 컬럼 `SYSCAT.COLUMNS … ORDER BY COLNO` |
| 뷰 / MQT | `…/cache/DB2ViewCache.java` · `DB2MaterializedQueryTableCache.java` | `SYSCAT.TABLES T, SYSCAT.VIEWS V` 조인 · TYPE `('V','W')` / `('S')` |
| 별칭 | `…/cache/DB2AliasCache.java` | TABLES TYPE='A' ∪ SEQUENCES SEQTYPE='A' ∪ MODULES MODULETYPE='A' |
| 닉네임 | `…/cache/DB2NicknameCache.java` | `SYSCAT.NICKNAMES` |
| PK/UK | `…/cache/DB2TableUniqueKeyCache.java` | `TABCONST C, KEYCOLUSE KCU … TYPE IN ('P','U') ORDER BY CONSTNAME, COLSEQ` |
| FK / 역참조 | `…/cache/DB2TableForeignKeyCache.java` · `DB2TableReferenceCache.java` | `REFERENCES R, KEYCOLUSE KCU` |
| 체크 | `…/cache/DB2TableCheckConstraintCache.java` | `CHECKS C, COLCHECKS CK` |
| 인덱스 | `…/cache/DB2IndexCache.java` | `SYSCAT.INDEXES WHERE INDSCHEMA=?` + `SYSCAT.INDEXCOLUSE … ORDER BY COLSEQ` |
| 루틴 | `…/cache/DB2RoutineCache.java` · `DB2RoutineParmsCache.java` | `ROUTINES WHERE ROUTINESCHEMA=? AND ROUTINETYPE='%s' AND ROUTINEMODULENAME IS NULL` · `ROUTINEPARMS WHERE ROUTINESCHEMA=? AND SPECIFICNAME=? ORDER BY ORDINAL` |
| 트리거 | `…/cache/DB2TriggerCache.java` | `SYSCAT.TRIGGERS WHERE TRIGSCHEMA=?` |
| 시퀀스 · 모듈 · 패키지 · 타입 | `…/model/DB2Schema.java` | `SEQUENCES … SEQTYPE <> 'A'` · `MODULES … MODULETYPE <> 'A'` · `PACKAGES` · `DATATYPES METATYPE <> 'S'` · 시스템 스키마 목록 |
| 소스 | `DB2Routine.java`·`DB2Trigger.java`(`TEXT` 열) · `DB2View`(VIEWS.TEXT) · `DB2Table.getObjectDefinitionText` → `DB2Utils.generateDDLforTable`(DB2LK) |
| 행 수 · 크기 | `DB2Table.java`(`CARD`) · `DB2Schema.collectObjectStatistics`(`ADMIN_GET_TAB_INFO`) |
| 현재 스키마 | `…/model/DB2ExecutionContext.java` | `VALUES(CURRENT SCHEMA)` · `SET CURRENT SCHEMA = %s` |
| 세션 관리 | `…/model/app/DB2ServerApplicationManager.java` · `DB2Utils.SEL_APP` | `SELECT * FROM SYSIBMADM.APPLICATIONS WITH UR` · 종료 = `CALL SYSPROC.ADMIN_CMD('FORCE APPLICATION (%s)')` · 권한 확인 = `DB2CurrentUserPrivileges.java` |

### 그리드 편집 키
- `…/DB2Constants.java` `PSEUDO_ATTR_RID_BIT` = `DBDPseudoAttribute(ROWID, "RID_BIT()", "RID_BIT($alias)", …)` · `DB2Table.getPseudoAttributes()`가 `isAtLeastV9_5() && !isBigSQL()`일 때만 제공(BigSQL은 SQLCODE -5115).
- 키 선택 순서(PK → 유일 키 → 가상 키/ROWID)는 DBeaver 공통 코어 규칙(추정 — db2 플러그인 밖 `DBDRowIdentifier` 로직).
- 우리 쪽 함의: `RID_BIT()`는 REORG 뒤 바뀔 수 있어 **편집 직전 조회값만** 사용 + 1행 사전 검사 유지(추정 — RID 안정성은 IBM 문서 확인 필요).

### 주의(DBeaver 쪽 오기)
`…/model/dict/DB2TableType.java`는 `U`를 "Inoperative"로 적지만 IBM 문서상 **U = 타입드 테이블**(비작동 표시는 `STATUS`). 우리는 IBM 정의를 따른다.

출처: <https://github.com/dbeaver/dbeaver/tree/devel/plugins/org.jkiss.dbeaver.ext.db2> (위 파일들 · 2026-10-25 얕은 복제로 직접 읽음)

## 추정 항목
1. ADMIN_CMD로 FORCE APPLICATION을 부를 때 SYSADM/SYSCTRL/SYSMAINT가 추가로 필요한지(명령 문서는 필요 · ADMIN_CMD 문서는 EXECUTE만 명시).
2. `MON_GET_CONNECTION`·`MON_GET_ACTIVITY` 권한(모니터 함수 공통 규칙으로 추정 · 개별 페이지 미확인).
3. `CHECKS.TYPE`의 시스템 생성 제약 걸러내기 필요성.
4. LIKE의 match-expression = CLOB 허용(문서는 금지하지 않음 · 금지는 pattern 쪽만) — 실서버 1회 확인 권장.
5. 식 기반 인덱스의 INDEXCOLUSE.COLNAME이 시스템 생성 이름(`K00…`)으로 나오는지.
6. `SYS%` 접두 전체 숨김 규칙.
7. DBeaver 키 선택 순서(코어 공통 로직 · db2 플러그인에서 직접 확인 안 함).
8. `RID_BIT()` 값의 REORG 후 안정성.
9. Db2 12.1에서 위 뷰·코드 변경 없음(11.5 문서로 확인 · 12.1은 개별 확인 안 함).

---
## ④ 오류 · 문법 · EXPLAIN (IBM Db2 LUW 11.5 / 12.1 · CLI/ODBC 경유)

**결론**
- 오류 분류는 **SQLCODE를 1차 키**, SQLSTATE를 2차 키로 쓴다. 같은 SQLCODE라도 이유 코드에 따라 의미가 갈린다(-911 RC 2 = 교착 · RC 68 = 잠금 시간 초과). 이유 코드는 `SQLERRMC` 토큰에 들어 있다.
- **Db2 LUW는 오류 위치(줄·열·오프셋)를 돌려주지 않는다.** SQL0104N의 `"토큰"`과 `following "앞 글"`을 본문에서 찾아 위치를 추정하는 방식밖에 없다.
- EXPLAIN은 `SYSPROC.SYSINSTALLOBJECTS('EXPLAIN','C',NULL,NULL)`로 SYSTOOLS에 explain 테이블을 만든다. 그다음 `EXPLAIN PLAN SET QUERYNO=n FOR …`를 실행하고, EXPLAIN_STREAM × EXPLAIN_OPERATOR 재귀 SQL로 비용이 붙은 트리를 직접 만든다. 실제 실행 계획(actuals)은 이벤트 모니터가 필요하므로 2차로 미룬다.

### ④-1 오류 분류표

| 군 | SQLCODE | SQLSTATE | 뜻 (이유 코드) | 제안 분류 |
|---|---|---|---|---|
| 권한 | -551 · -552 · -1060 | 42501 · 42502 · 08004 | 객체 권한 없음 · 연산 권한 없음 · CONNECT 권한 없음 | Auth/Permission |
| 인증 | -30082 | 08001 | 보안 처리 실패. RC 24 = 사용자/암호 무효, RC 1 = 암호 만료 | Auth (접속 단계) |
| 객체 없음 | -204 · -440 · -1013 | 42704 · 42884 · 42705 | 정의되지 않은 이름 · 루틴 없음(시그니처 불일치) · DB 별칭 없음 | NoObject |
| 컬럼 없음 | -206 | 42703 | 문맥상 유효하지 않은 컬럼 이름 | NoColumn |
| 중복 | -803 · -601 | 23505 · 42710 | 고유키 위반(SQLERRMC = 인덱스 ID) · 이미 있는 객체 이름 | Duplicate |
| 구문 | -104 · -199 · -007 | 42601 | 예상치 못한 토큰 · 예약어 사용 위치 오류 · 허용되지 않는 문자 | Syntax |
| 잠금 | -911 | 40001 | **트랜잭션 전체 롤백**. RC 2 = 교착, RC 68 = 잠금 시간 초과 | Lock/Deadlock (롤백됨을 표시) |
| 잠금 | -913 | 57033 | 같은 RC 2/68이지만 **롤백되지 않는다**(문장 하나만 실패) | Lock/Timeout |
| 접속 | -30081 · -1024 · -30108 | 08001 · 08003 · 08506 | 통신 오류(프로토콜·함수·rc 토큰) · 연결 없음 · 클라이언트 재경로(재시도 대상) | Connection |
| 취소 | -952 | 57014 | 인터럽트로 처리 취소(SQLCancel · 쿼리 시간 초과) | Cancelled |
| 제약 | -530 · -532 · -545 | 23503 · 23504 · 23513 | FK 부모 없음 · 자식 행이 남아 있어 삭제 거부 · CHECK 위반 | Constraint |
| NOT NULL | -407 | 23502 | NOT NULL 컬럼에 NULL 대입(SQLERRMC = TBSPACEID/TABLEID/COLNO) | NotNull |
| 길이 초과 | -302 · -433 · -404 | 22001 (숫자는 22003) | 호스트 변수 값이 너무 큼 · 값이 너무 긺 · 문자열 컬럼 길이 초과 | DataTooLong |
| 변환 | -420 · -180/-181 · -305 | 22018 · 22007/22008 · 22002 | 문자를 숫자로 바꿀 수 없음 · 날짜/시간 형식·값 오류 · 지시자 없는 NULL | Conversion |
| 0 나눗셈·산술 | -802 | 22003 · 22012 | 산술 오버플로/0 나눗셈. 종류는 SQLERRMC 토큰에 있음 | DivideByZero / Arithmetic |
| 로그 가득 | -964 | 57011 | 트랜잭션 로그가 가득 참 | Resource (TxLogFull) |
| 자원 | -968 · -954 · -1224 | 57011 · 57011 · 55032 | 파일 시스템 가득 · 애플리케이션 힙 부족 · 에이전트 시작 실패/강제 종료 | Resource |
| 사용자 오류 | -438 | 사용자 지정 | SIGNAL/RAISE_ERROR로 일으킨 오류 | Unknown (메시지 그대로) |
| 데이터 없음 | +100 | 02000 | 오류가 아님(행 없음) | — |

- -911과 -913은 SQLCODE가 아니라 **이유 코드(SQLERRMC 토큰)로 교착과 시간 초과를 가른다**. CLI에서는 `SQLGetDiagRec` 메시지 본문의 `Reason code "68"`을 정규식으로 읽는다.
- 그 밖의 메시지는 기본적으로 Unknown이다. 단 SQLSTATE 앞 두 글자로 대분류할 수 있다: `08` → Connection · `23` → Constraint · `22` → Data · `42` → Syntax/Name · `40` → Rollback · `57` → Resource.

### ④-2 오류 위치

- 메시지 형식은 `SQL0104N An unexpected token "<token>" was found following "<text>". Expected tokens may include: "<list>". SQLSTATE=42601`이다. SQL0199N도 `The use of the reserved word "<kw>" following "<text>" is not valid.`로 같은 꼴이다.
- **위치 필드는 없다.**
  - LUW SQLCA의 `sqlerrd(5)`는 제약·트리거 영향 행 수이고, 오류 때는 내부 오류 포인터다. 오프셋이 아니다(Informix의 `sqlerrd[4]`=오프셋과 혼동 주의).
  - CLI의 `SQLGetDiagField`에도 문장 안 위치 필드가 없다(추정 — ODBC 표준 진단 필드에 위치 항목이 없음).
  - `SQLGetSQLCA()`는 이제 쓰지 않는 것이 권장된다.
- `SQLERRMC`는 토큰을 0xFF로 구분한다(-104라면 token · text · list). 다만 CLI 메시지 본문에서 따옴표 안을 뽑는 편이 간단하다.
- **권장 추출 전략**
  1. 메시지에서 `token`과 `following "text"`를 뽑는다.
  2. 원문에서 `text`(최대 약 20자, 앞쪽이 잘릴 수 있음)를 찾고, 그 **뒤에 오는 첫 `token`** 위치를 오류 위치로 삼는다.
  3. `text`가 여러 번 나오면 원문을 정규화(공백 압축)한 뒤 마지막 일치를 쓴다. 없으면 `token`의 첫 등장 위치를 쓴다.
- **함정**
  - `following`은 Db2가 **토큰화한 뒤의 글**이다. 공백·주석이 빠지거나 대문자로 바뀌어 있을 수 있다.
  - 토큰이 `END-OF-STATEMENT`·`<END-OF-STATEMENT>`(문장 끝)일 수 있다.
  - 메시지는 클라이언트 로캘로 번역된다(한국어 메시지). 그러면 영어 정규식이 실패한다 → **CLI 접속 키워드로 영어 메시지를 강제**하거나(추정) 따옴표 패턴만 쓴다.
  - SQL PL 본문 안에서 난 오류는 `LINE NUMBER=n`을 줄 때가 있다(SQL20000계열 · 추정). 이것은 루틴 본문 기준 줄 번호다.

### ④-3 방언 특징(스크립트 분리기·문법용)

| 항목 | 내용 |
|---|---|
| 행 제한 | `FETCH FIRST n ROWS ONLY` · `OFFSET n ROWS FETCH NEXT m ROWS ONLY`(11.1+) · **`LIMIT n OFFSET m`은 11.1부터 항상 활성**(호환 벡터 bit 15 0x4000은 폐기·무시). `LIMIT m,n`(MySQL꼴)도 허용(추정) |
| 더미 테이블 | `SYSIBM.SYSDUMMY1` · `VALUES expr`(단독 문장 · 결과셋을 돌려줌) · `DUAL`은 호환 벡터 bit 2(0x02, ORA) |
| 식별자 | 따옴표 없는 이름은 **대문자로 접힌다**. `"Mixed"`는 대소문자 유지. 최대 128바이트 |
| 터미네이터 | 기본은 `;`. CLP는 `db2 -td@ -vf f.sql`로 바꾸거나 스크립트 안에 `--#SET TERMINATOR @` 지시문을 쓴다(줄 머리 주석). Data Studio도 같은 지시문을 이해한다(추정). DBeaver는 BEGIN…END 블록을 파서로 인식하고, 그 밖은 "빈 줄 = 구분자"·사용자 지정 구분자 설정(추정) |
| 분리 규칙 | `CREATE [OR REPLACE] PROCEDURE/FUNCTION/TRIGGER … BEGIN … END`와 `BEGIN ATOMIC … END`는 안쪽 `;`에서 끊지 않는다 → BEGIN/END 깊이 추적. `CASE…END`·`IF…END IF`·`LOOP…END LOOP`의 END도 깊이에 넣는다. `--#SET TERMINATOR x`를 만나면 구분자를 바꾼다(우리 분리기도 지원 권장) |
| 전역 변수 | `CREATE [OR REPLACE] VARIABLE s.v INTEGER DEFAULT 0` · 값 대입은 `SET s.v = …` · 세션마다 값이 따로 산다 |
| 스키마 | `SET SCHEMA x` = `SET CURRENT SCHEMA x` = `SET CURRENT SQLID`(동의어) · 경로는 `SET CURRENT PATH` |
| 자동 커밋 | CLI/ODBC 기본은 **ON**(`SQL_ATTR_AUTOCOMMIT`) · CLP도 기본 ON(`+c`로 끔) |
| 결과셋 | `CREATE PROCEDURE … DYNAMIC RESULT SETS n` + `DECLARE c CURSOR WITH RETURN [TO CALLER/CLIENT]` · CLI는 `CALL` 뒤에 `SQLMoreResults`로 차례로 받는다 |
| 복합문 | `BEGIN ATOMIC … END`(인라인 SQL PL · 동적 실행 가능) · 프로시저 본문은 `BEGIN [NOT ATOMIC] … END` |
| MERGE | `MERGE INTO t USING (…) AS s ON … WHEN MATCHED [AND …] THEN UPDATE … WHEN NOT MATCHED THEN INSERT … [ELSE IGNORE]` |
| 격리 절 | 문장 끝에 `WITH UR / CS / RS / RR`(SELECT · 일부 DML) · `FOR READ ONLY` · `FOR UPDATE OF` |
| 특수 레지스터 | `CURRENT DATE/TIME/TIMESTAMP` · `CURRENT USER`(= `USER` · `SESSION_USER`) · `CURRENT SERVER` · `CURRENT SCHEMA` · `CURRENT ISOLATION` · `CURRENT TIMEZONE` · `CURRENT EXPLAIN MODE`. 공백 대신 밑줄을 쓴 꼴(`CURRENT_DATE`)도 허용 |
| 연결 | `||` = `CONCAT`(중위 `CONCAT`도 가능) |
| BOOLEAN | 11.1.1.1부터 컬럼형으로 지원(그 전에는 SQL PL 안에서만) · `TRUE/FALSE` 리터럴 |
| 특수 형식 | `DECFLOAT(16/34)` · `GRAPHIC/VARGRAPHIC/DBCLOB`(2바이트 문자) · `XML` · `CHAR … FOR BIT DATA`(바이너리) · `BINARY/VARBINARY`(10.5+) |
| Oracle 호환 | `DB2_COMPATIBILITY_VECTOR=ORA`(bit 1 ROWNUM · 2 DUAL · 4 CONNECT BY · 5 NUMBER · 6 VARCHAR2 · 7 DATE→TIMESTAMP(0) · 12 PL/SQL 컴파일 · 18 DB 링크) · **인스턴스 재시작 + DB를 새로 만들어야** 일부가 적용된다 · 앱은 `SYSIBMADM.DBCFG`/`DB2_COMPATIBILITY_VECTOR` 값을 보고 문법을 켜야 함 |

### ④-4 EXPLAIN

1. **테이블 생성**: `CALL SYSPROC.SYSINSTALLOBJECTS('EXPLAIN', 'C', CAST(NULL AS VARCHAR(128)), CAST(NULL AS VARCHAR(128)))`
   - 시그니처는 `(tool-name, action, tablespace-name, schema-name)`이다. action은 C(생성)·D(삭제)·V(검증)·M(이주) 넷이다.
   - 테이블스페이스가 NULL이면 SYSTOOLSPACE(없으면 만든다), 스키마가 NULL이면 **SYSTOOLS**다. 사용자 스키마에 만들려면 4번째 인자에 `CURRENT USER`를 넘긴다.
   - **스키마 결정 순서**: explain 테이블을 찾을 때 현재 권한 ID 스키마가 먼저이고 SYSTOOLS가 다음이다(추정).
2. **실행**: `EXPLAIN PLAN [SET QUERYNO = n] [SET QUERYTAG = 'nsql'] FOR <문장>`
   - `EXPLAIN ALL`은 `EXPLAIN PLAN`과 동의어다. 문장은 실제로 실행되지 않는다.
   - `?` 매개변수 마커가 있어도 된다.
   - 대안은 `SET CURRENT EXPLAIN MODE EXPLAIN`이다(이 경우 실행하지 않고 기록만 한다).
3. **주요 테이블**
   - EXPLAIN_INSTANCE: 머리
   - EXPLAIN_STATEMENT: `EXPLAIN_LEVEL` O = 원문, P = 계획 · `TOTAL_COST` · `STATEMENT_TEXT` · `QUERYNO` · `QUERYTAG`
   - EXPLAIN_OPERATOR: `OPERATOR_ID` · `OPERATOR_TYPE`(TBSCAN·IXSCAN·FETCH·NLJOIN·HSJOIN·MSJOIN·SORT·GRPBY·RETURN…) · `TOTAL_COST` · `IO_COST` · `CPU_COST` · `FIRST_ROW_COST`
   - EXPLAIN_STREAM: `SOURCE_TYPE/TARGET_TYPE` O = 연산자, D = 데이터 객체(D일 때 ID = -1) · `SOURCE_ID/TARGET_ID` · `STREAM_COUNT` = 추정 행 수 · `OBJECT_SCHEMA/NAME`
   - EXPLAIN_OBJECT: 테이블·인덱스와 통계
   - EXPLAIN_ARGUMENT: 연산자 인자(조인 방식·스캔 방향 등)
   - EXPLAIN_PREDICATE: 술어
   - 공통 키 6개는 `EXPLAIN_REQUESTER, EXPLAIN_TIME, SOURCE_NAME, SOURCE_SCHEMA, SOURCE_VERSION, EXPLAIN_LEVEL` + `STMTNO, SECTNO`다.
4. **db2exfmt 없이 트리 읽기**(권장 골격). 우리 세션에서 방금 만든 행만 대상으로 하므로, `EXPLAIN_TIME = (SELECT MAX(EXPLAIN_TIME) FROM EXPLAIN_STATEMENT WHERE QUERYTAG='nsql' AND EXPLAIN_REQUESTER=CURRENT USER)`로 고정한다.
   ```sql
   WITH t(id, parent, lvl) AS (
     SELECT o.OPERATOR_ID, CAST(NULL AS INT), 0 FROM EXPLAIN_OPERATOR o
      WHERE o.OPERATOR_TYPE='RETURN' AND <키 일치>
     UNION ALL
     SELECT s.SOURCE_ID, t.id, t.lvl+1 FROM t JOIN EXPLAIN_STREAM s
       ON s.TARGET_TYPE='O' AND s.TARGET_ID=t.id AND s.SOURCE_TYPE='O' AND <키 일치>
   )
   SELECT t.lvl, t.id, t.parent, o.OPERATOR_TYPE, o.TOTAL_COST, o.IO_COST, o.CPU_COST,
          (SELECT MAX(STREAM_COUNT) FROM EXPLAIN_STREAM x WHERE x.SOURCE_ID=t.id AND x.SOURCE_TYPE='O' AND <키>) AS est_rows,
          (SELECT LISTAGG(x.OBJECT_SCHEMA||'.'||x.OBJECT_NAME, ',') FROM EXPLAIN_STREAM x
            WHERE x.TARGET_ID=t.id AND x.SOURCE_TYPE='D' AND <키>) AS objects
   FROM t JOIN EXPLAIN_OPERATOR o ON o.OPERATOR_ID=t.id AND <키>;
   ```
   - 재귀 CTE에서 Db2는 SQL0347W(무한 재귀 가능 경고)를 낼 수 있다. 무시해도 된다.
   - 트리 구성(자식 순서 = `STREAM_ID`)과 표시는 클라이언트가 맡는다.
   - 인자 상세는 EXPLAIN_ARGUMENT(`ARGUMENT_TYPE`/`ARGUMENT_VALUE`/`LONG_ARGUMENT_VALUE`)로 지연 로딩한다.
5. **EXPLAIN_FORMAT_STATS**: EXPLAIN_STATEMENT.`SNAPSHOT`(BLOB)을 CLOB 통계 문으로 바꾸는 스칼라 함수다. 스냅숏은 `EXPLAIN PLAN WITH SNAPSHOT FOR …`이 있어야 채워진다. 11.5.9 이후 도입이라는 자료가 있어 버전 확인이 필요하다(추정). 1차에서는 쓰지 않는다.
6. **실제 실행 계획(section actuals)**
   - 조건: `section_actuals` DB 설정(또는 `WLM_SET_CONN_ENV`로 연결 단위 BASE) + 활동 이벤트 모니터 `COLLECT ACTIVITY DATA WITH SECTION`.
   - 절차: 실행 → `EXPLAIN_FROM_ACTIVITY(appl_id, uow_id, activity_id, evmon, …)` → EXPLAIN_ACTUALS 테이블.
   - 패키지 캐시에서 추정 계획만 가져오려면 `EXPLAIN_FROM_SECTION`을 쓴다.
   - **권장**: 1차는 추정 계획(EXPLAIN PLAN)만 낸다. actuals는 DBA 권한·이벤트 모니터가 필요하므로 2차 옵션으로 둔다. 기능을 켤 때만 이벤트 모니터를 만들고 끝나면 지운다.
   - 권한: EXPLAIN 권한(또는 DBADM) + explain 테이블 INSERT 권한이 있어야 한다.

### ④-5 예약어 · 내장 함수 목록 원천

- **예약어**
  - Db2 LUW는 "예약어라도 대부분 식별자로 쓸 수 있지만, 키워드로 읽힐 자리에서는 따옴표가 필요하다"는 입장이다. 공식 목록은 SQL 표준과 비교한 표로 제공된다.
  - 예약 스키마: SYSCAT · SYSFUN · SYSIBM · SYSIBMADM · SYSPROC · SYSPUBLIC · SYSSTAT. SYS로 시작하는 스키마는 금지(42939). SESSION은 DGTT용이다.
  - 대표 예약어: `ALL ALTER AND ANY AS ASC BEGIN BETWEEN BY CALL CASE CAST CHECK COLUMN COMMIT CONNECT CREATE CURRENT CURSOR DECLARE DEFAULT DELETE DESC DISTINCT DROP ELSE END ESCAPE EXCEPT EXISTS EXPLAIN FETCH FOR FOREIGN FROM FULL FUNCTION GRANT GROUP HAVING IF IN INNER INSERT INTERSECT INTO IS JOIN KEY LEFT LIKE LIMIT MERGE NOT NULL OF OFFSET ON OR ORDER OUTER PRIMARY PROCEDURE REFERENCES RETURN REVOKE RIGHT ROLLBACK ROWNUM SELECT SET SIGNAL TABLE THEN TO TRIGGER UNION UNIQUE UPDATE USER USING VALUES VARIABLE WHEN WHERE WITH`
- **내장 함수**: 공식 "Built-in functions" 페이지는 집계·스칼라·테이블 함수를 분류해 나열한다. SYSIBM 함수 외에 SYSFUN·SYSPROC·SYSIBMADM 모듈 함수도 별도다.
  - 집계 대표: `AVG COUNT COUNT_BIG LISTAGG MAX MIN STDDEV SUM VARIANCE ARRAY_AGG CORRELATION COVARIANCE MEDIAN PERCENTILE_CONT XMLAGG`
  - 스칼라 대표: `ABS CEIL COALESCE CONCAT DATE DAYS DECIMAL DECODE DIGITS HEX INSTR LCASE LEFT LENGTH LOCATE LOWER LPAD LTRIM MOD NULLIF NVL POSSTR REPLACE RIGHT ROUND RTRIM SUBSTR SUBSTRING TIMESTAMP TIMESTAMPDIFF TO_CHAR TO_DATE TRANSLATE TRIM TRUNCATE UCASE UPPER VALUE VARCHAR VARCHAR_FORMAT YEAR`
- **문법 파일 권장**: 목록은 위 공식 페이지에서 한 번 추출해 정적 파일로 박아 둔다. 접속 중에는 `SYSCAT.ROUTINES WHERE ROUTINESCHEMA IN ('SYSIBM','SYSFUN')`으로 보충한다(지연 로딩).

### 출처
- SYSINSTALLOBJECTS: https://ibm.com/docs/SSEPGG_11.5.0/com.ibm.db2.luw.sql.rtn.doc/doc/r0011876.html
- DB2_COMPATIBILITY_VECTOR(비트표 · LIMIT/OFFSET 상시 활성): https://www.ibm.com/support/knowledgecenter/en/SSEPGG_11.1.0/com.ibm.db2.luw.apdv.porting.doc/doc/r0052867.html
- Explain 테이블 구성: https://ibm.com/docs/SSEPGG_11.5.0/com.ibm.db2.luw.admin.perf.doc/doc/c0005137.html · EXPLAIN_STREAM: https://www.ibm.com/docs/en/SSEPGG_9.7.0/com.ibm.db2.luw.sql.ref.doc/doc/r0001103.html
- EXPLAIN_FORMAT_STATS: https://www.ibm.com/docs/en/SSEPGG_12.1.0/com.ibm.db2.luw.sql.rtn.doc/doc/r0052902.html · EXPLAIN_ACTUALS: https://www.ibm.com/docs/en/SSEPGG_12.1.0/com.ibm.db2.luw.sql.ref.doc/doc/r0056361.html · EXPLAIN_FROM_ACTIVITY: https://www.ibm.com/docs/en/SSFMBX/com.ibm.swg.im.dashdb.sql.rtn.doc/doc/r0056203.html · 활용 예: https://ibm.com/docs/SSEPGG_11.1.0/com.ibm.db2.luw.admin.mon.doc/doc/t0058655.html
- SQLCA(sqlerrd(5) 정의): https://www.ibm.com/support/knowledgecenter/SSEPGG_12.1.0/com.ibm.db2.luw.sql.ref.doc/doc/r0002212.html
- 예약어·예약 스키마: https://www.ibm.com/docs/en/SSHRBY/com.ibm.swg.im.dashdb.sql.ref.doc/doc/r0001095.html (11.5 판 = 같은 토픽 r0001095 · 버전 선택기)
- -911 RC 2/68: https://www.ibm.com/support/pages/node/500181 · https://www.ibm.com/support/pages/db2-online-backup-fails-sql0911n-reason-code-68
- 메시지 색인(SQL0000–SQL0249 등): https://www.ibm.com/docs/en/db2/11.5.x?topic=messages-sql0000-sql0249 · 실행 계획 입문: https://use-the-index-luke.com/sql/explain-plan/db2/getting-an-execution-plan

### 추정 항목
1. -30082 RC 24/1의 정확한 뜻, -1224 = 55032, -1013 = 42705, -404·-180·-305의 SQLSTATE(기억 기반 · 11.5 메시지 페이지에서 직접 확인하지 못함).
2. CLI `SQLGetDiagField`에 문장 안 위치 필드가 없다는 것. 영어 메시지 강제 방법. SQL PL 오류의 `LINE NUMBER`.
3. `LIMIT m,n`(MySQL꼴) 허용 여부.
4. Data Studio/DBeaver의 `--#SET TERMINATOR` · 블록 인식 동작.
5. explain 테이블 스키마 결정 순서(권한 ID → SYSTOOLS).
6. EXPLAIN_FORMAT_STATS 도입 버전(11.5.9).
7. `EXPLAIN PLAN WITH SNAPSHOT` 구문.
8. SQL0347W 재귀 경고 번호.
9. `ROWNUM`·`LIMIT`·`OFFSET`·`VARIABLE`·`EXPLAIN`이 공식 예약어 표에 들어 있는지(대표 목록은 기억 기반 · 문법 파일은 공식 표에서 다시 추출할 것).
10. 11.5용 메시지·예약어·내장 함수 페이지 URL의 topic slug.
11. ④-검증의 -668(57016)·-613(54008)·-444(42724)·-1032(57019)·-1403(08004) 뜻, SAVEPOINT의 `ON ROLLBACK RETAIN CURSORS` 필수 여부, `` 인자 동작, 파생 테이블 ORDER BY 보존.

### ④-검증 (팀 초안 `db2-errmap.txt` · `db2-misc.txt` 대조)

**검증 요약**
- **틀린 매핑 2개**: -668과 -613. **주의 6개**: -204 범위, -444, -1013, -911 단일 분류, EXPLAIN 테이블 스키마, SYSINSTALLOBJECTS의 `''` 인자.
- **문법은 맞다**: `EXPLAIN PLAN SELECTION FOR …`는 LUW에서 유효하고 `ALL`과 동의어다. `OFFSET … ROWS FETCH FIRST … ROWS ONLY`는 11.1 이상에서 된다.

| 항목 | 판정 | 근거 URL | 고칠 점 |
|---|---|---|---|
| -204 → NoTable | 주의 | 메시지 색인 ①¹ | SQL0204N은 "정의되지 않은 이름"이다. 테이블뿐 아니라 인덱스·시퀀스·별칭·스키마·트리거 등 모든 객체에 쓰인다. NoObject가 맞다(테이블 문맥일 때만 NoTable) |
| -1013 → NoObject | 주의 | ①¹ | DB 별칭/이름을 찾지 못함(접속 단계 · 42705)이다. **Connection**(접속 실패) 쪽이 사용자 안내에 맞다 |
| -206 → NoColumn | ✓ | ①¹ | — |
| -440 → NoObject | ✓ | ①¹ | 이름은 있으나 인자 수·형식이 맞지 않는 경우(42884)도 포함한다. 안내 문구에 "인자 수·형식 확인"을 넣는다 |
| -444 → NoObject | 주의 | ①¹ | SQL0444N은 외부 루틴의 **라이브러리·진입점에 접근할 수 없음**(42724 · 이유 코드 동반)이다. 객체 없음이 아니라 서버 설치 문제다 → Unknown/Resource 권장 |
| -104 · -199 · -10 → Syntax | ✓ | ①¹ | -10 = 문자열 상수가 닫히지 않음(42603). -7(허용되지 않는 문자 · 42601)을 추가할 것 |
| -30082 · -1403 → Login | ✓ | ①¹ | -1403 = 사용자 이름/암호 틀림(08004). -30082는 이유 코드(24 = 암호/사용자 무효, 1 = 만료 등)를 SQLERRMC에서 꺼내 안내에 덧붙인다 |
| -551 · -552 → Permission | ✓ | ①¹ | -1060(CONNECT 권한 없음 · 08004)을 추가할 것 |
| -803 → Unique | ✓ | ①¹ | 23505. SQLERRMC = `인덱스ID` 또는 테이블 이름이다. 객체 추출은 문장 기반이 맞다 |
| -530 · -532 → ForeignKey | ✓ | ①¹ | 23503 · 23504(-532 = 자식 행이 남아 삭제 거부) |
| -407 → NotNull | ✓ | ①¹ | 23502. 토큰은 `TBSPACEID=…, TABLEID=…, COLNO=…` 꼴이라 **이름 따옴표가 없을 수 있다** → obj_q 실패 시 None |
| -545 → Check | ✓ | ①¹ | 23513 |
| -911 → Deadlock | 주의 | https://www.ibm.com/support/pages/node/500181 | RC 2 = 교착, **RC 68 = 잠금 시간 초과**. 둘 다 **트랜잭션 전체가 롤백**된다(40001). 이유 코드로 Deadlock/LockTimeout을 가르고, "트랜잭션이 롤백됨" 안내를 붙일 것 |
| -913 → Lock | ✓ | ①¹ | 57033 · 같은 RC 2/68이지만 롤백하지 않는다(문장만 실패) |
| -668 → Lock | **✗** | ①¹ (SQL0500–0749 범위) | SQL0668N = "테이블에 대해 이유 코드 n 때문에 연산이 허용되지 않음"(57016)이다. RC 1 = SET INTEGRITY 보류, RC 7 = **REORG 보류** 등. 잠금이 아니다 → **ObjectState/Unknown** 같은 새 분류 또는 Unknown + 이유 코드 안내 |
| -30081 · -1224 · -1024 · -1032 → Connection | ✓ | ①¹ | -1032 = DB 관리자 미기동(57019). -30108(클라이언트 재경로 · 08506)도 추가 |
| -420 · -433 · -302 → DataType | ✓ | ①¹ | 22018 · 22001 · 22001/22003. -802(산술 오류/0 나눗셈 · 22003/22012)과 -180/-181(날짜 형식) 추가 권장 |
| -613 → DataType | **✗** | ①¹ (SQL0500–0749) | SQL0613N = 기본키/고유키/분할키가 **너무 길거나 컬럼이 너무 많음**(54008)으로, DDL 한도 오류다 → Unknown(또는 Limit) |
| 누락: -952 · -964 · -968 · -954 · -601 | 주의 | ①¹ | 취소(57014) · 로그 가득(57011) · 디스크 가득 · 힙 부족 · 이름 중복(42710 → Duplicate)을 추가 |
| `None if has("undefined name")` 폴백 | ✓ | — | 한국어 로캘 메시지에서는 실패한다(추정). SQLSTATE 42704 폴백을 함께 둔다 |
| `EXPLAIN PLAN SELECTION FOR <stmt>` | ✓ | https://www.ibm.com/docs/en/SSEPGG_12.1.0/com.ibm.db2.luw.sql.ref.doc/doc/r0000952.html | 유효하다. `ALL` = `PLAN SELECTION`, `PLAN` = z/OS 호환용 동의어. `SET QUERYTAG='nsql'`을 덧붙여 우리 행을 식별하기를 권장 |
| EXPLAIN 후 `SELECT … FROM EXPLAIN_OPERATOR`(비한정) | 주의 | https://ibm.com/docs/SSEPGG_11.5.0/com.ibm.db2.luw.admin.perf.doc/doc/c0005137.html | 비한정 이름은 CURRENT SCHEMA로 풀린다. 테이블이 SYSTOOLS에 있으면 **-204**가 난다. EXPLAIN 기능이 쓴 스키마(권한 ID → SYSTOOLS 순 · 추정)와 같은 곳을 **명시 한정**해서 읽어야 한다 |
| `WHERE explain_time = (SELECT MAX(explain_time) FROM EXPLAIN_OPERATOR)` | 주의 | 같은 곳 | 다른 사용자·세션의 EXPLAIN과 섞인다. `EXPLAIN_REQUESTER = SESSION_USER`와 `QUERYTAG`(EXPLAIN_STATEMENT에서)로 고정하고, 키 6개(`EXPLAIN_REQUESTER, EXPLAIN_TIME, SOURCE_NAME, SOURCE_SCHEMA, SOURCE_VERSION, EXPLAIN_LEVEL` + `STMTNO, SECTNO`)로 조인한다 |
| 평면 목록(operator_id 순) vs 트리 | 주의 | EXPLAIN_STREAM: https://www.ibm.com/docs/en/SSEPGG_9.7.0/com.ibm.db2.luw.sql.ref.doc/doc/r0001103.html | 부모·자식 관계는 EXPLAIN_STREAM에만 있다(`SOURCE_TYPE/SOURCE_ID` → `TARGET_TYPE/TARGET_ID` · 'O' = 연산자, 'D' = 데이터 객체 · D이면 ID = -1 · `STREAM_COUNT` = 추정 행 수 · `OBJECT_SCHEMA/OBJECT_NAME`). 연산자 비용 컬럼은 `OPERATOR_TYPE, TOTAL_COST, IO_COST, CPU_COST, FIRST_ROW_COST`. 트리는 ④-4의 재귀 SQL로 만든다 |
| `SYSPROC.SYSINSTALLOBJECTS('EXPLAIN','C','','')` | 주의 | https://ibm.com/docs/SSEPGG_11.5.0/com.ibm.db2.luw.sql.rtn.doc/doc/r0011876.html | 시그니처 = `(tool-name VARCHAR(128), action CHAR(1), tablespace-name VARCHAR(128), schema-name VARCHAR(128))`. 문서는 **NULL을 넘기면 기본값**(SYSTOOLSPACE · SYSTOOLS)이라고만 하고 `''`의 동작은 문서에 없다 → **`NULL, NULL`**(CAST 불필요라고 명시됨)으로 바꾼다. 기본 스키마는 "현재 스키마"가 아니라 **SYSTOOLS**다 → 주석 수정. 우리 스키마에 만들려면 4번째 인자에 `CURRENT USER`(추정) |
| `SAVEPOINT x ON ROLLBACK RETAIN CURSORS` | ✓(주의) | https://www.ibm.com/docs/en/SSHRBY/com.ibm.swg.im.dashdb.sql.ref.doc/doc/r0003271.html | 문법은 `SAVEPOINT 이름 [UNIQUE] ON ROLLBACK RETAIN CURSORS [ON ROLLBACK RETAIN LOCKS]`. LUW에서는 `ON ROLLBACK RETAIN CURSORS`가 **필수**라고 판단한다(추정 · 페이지 텍스트로는 필수 표시를 확인하지 못함) → 항상 붙인다. `UNIQUE`는 같은 이름을 재사용하지 않겠다는 선언이므로 **붙이지 않는다**(재설정 시 3B501 위험). 되돌리기 = `ROLLBACK TO SAVEPOINT x` · 해제 = `RELEASE [TO] SAVEPOINT x` |
| `SELECT * FROM (q) x OFFSET n ROWS FETCH FIRST m ROWS ONLY` | ✓ | https://www.ibm.com/support/knowledgecenter/en/SSEPGG_11.1.0/com.ibm.db2.luw.sql.ref.doc/doc/r0061832.html · 호환 벡터: https://www.ibm.com/support/knowledgecenter/en/SSEPGG_11.1.0/com.ibm.db2.luw.apdv.porting.doc/doc/r0052867.html | offset-clause는 **11.1+**(10.5 이하 = ✗ → `ROW_NUMBER() OVER()` 폴백 필요). 같은 판부터 `LIMIT/OFFSET`도 상시 활성. ORDER BY가 없으면 페이지 경계가 비결정적이다 → 안쪽 q의 ORDER BY는 파생 테이블에서 보존이 보장되지 않으므로(추정) 바깥에 ORDER BY를 옮기거나 첫 페이지만 FETCH FIRST로 처리 |
| `scalar_from = SYSIBM.SYSDUMMY1` | ✓ | 예약 스키마 페이지(④-5) | `VALUES (expr)`도 단독 결과셋이 된다. 둘 다 가능 |

¹ ① = 메시지 색인 https://www.ibm.com/docs/en/db2/11.5.x?topic=messages-sql0000-sql0249 (같은 계열: `sql0250-sql0499` · `sql0500-sql0749` · `sql0750-sql0999` · `sql1000-sql1249` · `sql1250-sql1499` · `sql30000-sql30499` — topic slug 추정). -668·-613·-444·-1032·-1403의 뜻은 기억 기반이라 색인 본문으로 직접 확인하지 못했다(추정 항목 11).
