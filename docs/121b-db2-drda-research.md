# 121b — Db2 네이티브 DRDA 드라이버 조사 원문 (10-25 · 138차 Linux)

> **D-361 후보**(Db2 = 네이티브 DRDA 확장 드라이버 `nsql-driver-db2` · 순수 Rust · Oracle Thin과 같은 길 · 방언·SYSCAT·오류표는 [121a](121a-db2-research.md)·D-359 재사용 · 전송층만 교체 · ODBC 경로 = Windows 폴백)의 근거 자료. 배경 = 사용자 10-25 "Nexa SQL과 드라이버 파일만 있으면 실행 가능해야" + db2.yml 1~7차(Linux/macOS clidriver = 32비트 SQLLEN `libdb2.so`뿐 · 서버 설치본 `libdb2o.so`는 인스턴스 없이 ENV 할당 불가). 협업 세션(Opus)이 하위 에이전트 3개로 조사 = ① 프로토콜·FD:OCA · ②④ 보안·TLS·Db2 LUW 고유 · ③⑤ 순수 구현체·범위. 출처 = 표마다 URL · 직접 확인하지 못한 것은 **(추정)** · 각 절 끝 "추정 항목". 이 문서는 고치지 않는다(결론 SSOT = D-361과 소스). 참고 구현체는 **읽기만**(코드 복사 금지 · 라이선스 각 절).

> ★ **10-25 실측 정정(CI db2.yml 네이티브 2차 40115496236)** = PKGNAMCSN의 RDBNAM·RDBCOLID(`NULLID`)·PKGID(`SYSSH200`)는 **EBCDIC(cp500) 대문자**여야 한다(ASCII로 보내면 SQL0805N −805 · 51002 = 패키지 없음 · 이 문서의 "ASCII 바이트로 간다"는 SQL0805N 메시지의 `0X5359…` 표기에서 추론한 것이라 **일관성 토큰 8B(`SYSLVL01`)만** 해당 · b0bb8729) · 서버 실측 = PRDID `SQL12012` · TYPDEFNAM `QTDSQLX86` · CCSID 1208/1200/1208 · SECCHKCD 0(평문 SECMEC 3 인증 성공 = 컨테이너 기본 인증 추정 확인).

### 10-25 실측 규칙(CI db2.yml 네이티브 1~5차 · Db2 12.1 Community · 구현 = `nsql-driver-db2`)

| 항목 | 실측 | 근거 |
|---|---|---|
| 상관 번호 | **요청 DSS마다 +1** · 형식 바이트 0x10(같은 상관 = 다음이 객체)은 바로 뒤가 객체(OBJDSS)일 때만 · CNTQRY/CLSQRY = OPNQRY의 번호 | 1차 = EXCSAT·ACCSEC 둘 다 corr 1 + 0x51 → 서버 db2diag `sqljcReadHdrAs probe:20 · ZRC=0x80360086 SQLJC_ERROR_PRTCLE "PROTOCOL ERROR"` → a29897fd |
| 핸드셰이크 응답 | SECCHKCD 0(평문 SECMEC 3) · ACCRDBRM PRDID `SQL12012` · TYPDEFNAM `QTDSQLX86` · CCSID SBC 1208 · DBC 1200 · MBC 1208 · 서버 RDBNAM = EBCDIC 대문자 | 2차 40115496236 hexdump |
| PKGNAMCSN | RDBNAM·RDBCOLID(`NULLID`)·PKGID(`SYSSH200`) = **EBCDIC(cp500) 대문자 · 공백 채움** · 일관성 토큰 `SYSLVL01`만 ASCII | ASCII로 보내면 SQL0805N −805 · 51002(패키지 없음) → b0bb8729 |
| SQLCARD | 지시자 · SQLCODE 4B **작은 끝**(`DB FC FF FF` = −805) · SQLSTATE 5 · SQLERRPROC 8 · SQLERRD 영향 행 수 = INSERT 2행 → 2 | 2·3차 |
| SQLDARD | SQLCARD → SQLDIAGGRP `FF` → SQLDHGRP `FF` → **열 수 2B 작은 끝** → 열마다 PRECISION 2 · SCALE 2(작은 끝 · DECIMAL(10,2) = `0A 00`/`02 00`) · **LENGTH 8B 작은 끝** · **SQLTYPE 2B 작은 끝**(496 INTEGER · 449 VARCHAR null · 485 DECIMAL null · 385 DATE null · 448 VARCHAR) · **CCSID 2B 큰 끝** → 14B(8B 0 · 문자형 `08 00`/그 밖 `00 00` · 이름 없는 식 `00 01`/이름 열 `00 00` · **이름 길이 2B 큰 끝**) → 이름 바이트 → 0 길이 필드들 → `FF FF FF` 끝 표식 | 4차 덤프 3벌(`VALUES TRIM(CURRENT SCHEMA)` · `SELECT ID, NAME, AMT, D …` · 매개변수 질의) → 5757e608(끝 표식에서 역탐색 · 순수 시험) |
| 트레이스 | `NSQL_TRACE_DRDA=1` = 송수신 hexdump(512 B) · **SECCHK 체인은 통째 가림**("credentials masked") | 2차에 비밀번호 바이트 노출 지적 → b0bb8729 |
| LOB · 바인드 실행 | 비질의 바인드 실행 명령 = **EXCSQLSTT 0x200B**(0x2009 = ENDBND로 보내면 서버가 **PKGBNARM 0x2206** SVRCOD 8 · SRVDGN "AGENT: Invalid DDM Request" · SUBCODE2 = 2009) | db2.yml 40149213664(25017034) → 0c062e15 |
| 취소 | 실행 중 소켓 `shutdown(Both)` → 전송 오류 · `is_alive` 거짓 · 재접속 뒤 `VALUES 1` 정상 | 40149327666(f5cd3b83) 내장 ✅ · 확장 미확인 |
| ACCSECRD SECMEC | **반복 값 목록**(2B씩 · 예 `0008 11A2 0003 0005` = {3, 5}) + `SECCHKCD(0x11A4)` 0x01 = 요청 방식 미지원 → 한 값(`as_u16`)으로 읽으면 안 된다 | 40149595519(0c062e15) 4/4 접속 실패 |
| 인라인 LOB 자리표시 | QRYDSC 인라인 LOB 형식(0xC8~0xCF · 예 `CF 8009` CLOB · `C9 8009` BLOB)의 길이 `0x8009` 하위 = **QRYDTA 자리표시 9 B**(상태 1 + 길이 8 큰 끝 · 예 `02 0000000000000960` = 2400 B) · NULL = 지시자 `FF`만 · 값은 체인 뒤 EXTDTA 0x146C(상태 1 B + 바이트)로 행·열 순 | 40156275674 2행 누락(자리표시 0 B로 봄) → f57106aa |
| LOB 입력 | CLOB 바인드(409)를 NVARGRAPHIC 0x39로 보내면 −30020 · 58009 · DTAMCHRM 0x220E("0133") · SRVDGN "CMNMGR Read Failure" → `CLOB('…')`/`BLOB(X'…')` 리터럴 재작성(32 000 B 상한 · EXTDTA 입력은 2차 ⑥) | 40149862498 → 2ed0e60e |
| 결과 집합 끝 | CALL 집합은 ENDQRYRM 없이 **QRYDTA 행 자리의 SQLCA(지시자 00 · SQLCODE +100 · `02000`)** 로 끝난다 · 끝 뒤 CNTQRY는 SQLCA(+100 `SQLRR036`)만 계속 → 끝 판정 없으면 무한 루프 | 40150244681(로그 2.9 GB · 취소) → 8382bbfb(+ 진전 없는 CNTQRY 2회 중단 · 트레이스 4000줄) |
| CALL 응답 체인 | 한 체인(corr 같음) = RSLSETRM 0x2219{SVRCOD · PKGSNLST 0x2139 = PKGNAMCSN 68 B × 집합 수 · 집합마다 PKGSN 다름 · PKGID = 프로시저 패키지 `P…` · RDBCOLID = 스키마} → SQLDTARD 0x2413{FDODSC + FDODTA `FF 00 …`} → SQLRSLRD 0x240E(집합 수 2 B 작은 끝) → 집합마다 OPNQRYRM{QRYINSID} → SQLCINRD 0x240B → QRYDSC … · SQLCARD 없음 · 이어 받기 = PKGSNLST[i] + QRYINSID | 40150244681 · 40156275674 |
| FDODTA 지시자 | 최상위 비트 = 널 · IN 매개변수 자리는 `0x80`(값 없음) · OUT은 `00` + 값 | 40150244681 → 8382bbfb |
| SQLDTA FDODTA 행 지시자 | FDODTA 맨 앞 1 B = **행(그룹) 지시자 `0x00`이 늘 있어야 한다**(derby NULL_LID · SQLDTARD 읽기와 대칭) → 그 뒤 매개변수마다 널 지시자 + 값 · 예 `0014 147A | 00 | 00 01000000 | 00 02 0000000000001068` · 이전 구현은 이를 "홀수 길이 패드"로 보고 총 길이가 짝수면 빼서 첫 매개변수 지시자가 1 B 밀림 → DTAMCHRM −30020 "0133" · SRVDGN "CMNMGR Read Failure"(SUBCODE2 0x6B/0x6D) — 그전 바인드 통과는 총 길이가 홀수여서 우연 | 40183353540 판독 → 3d3482ad(회귀 단위 시험) → 40183700337 ✅ |
| SQLCINRD 열 레이아웃 | SQLDARD와 다름 = **SQLCA 자리 없음** · SQLDHGRP 지시자 `00`(널 아님) + 24 B(hold · return · scroll · sensitive · fcode `55 00` · keytype · RDBNAM/스키마 VC 길이) → 열 수 2 B 작은 끝 → 열마다 PRECISION·SCALE·LENGTH 8·SQLTYPE·CCSID(SQLDARD와 같음) → 15 B(0 ×8 · 문자형 `08 00` · `00 00` · SQLUNNAMED · 이름 길이 큰 끝) → 이름 → 라벨·설명 VC(0 ×10) → UDT `FF` → **SQLDXGRP `00`** = 12 B(keymem `01 00` = PK 열 …) + RDBNAM VCS + 상관명·기본 테이블·스키마·열 이름 VCM/VCS + 4 B → 꼬리 `FF` | 40156275674 열 이름 = 매개변수 이름(P_MIN·P_CNT)로 폴백 → 50023761 |
| 취소(확장 경로) | `CancelHandle::drops_session`이 cdylib 경계를 못 넘어 늘 거짓 → ABI v2 추가 op `CANCEL_DROPS_SESSION`(18 · out u8 · 함수표 불변 · 옛 드라이버 = 거짓) · SQL Server 확장도 같은 길 | 40157404807 확장 취소 ✗ → 41e1fe00 |
| LOB 입력 EXTDTA | 한 체인(corr 같음) = EXCSQLSTT `0x51` → SQLDTA `0x53`(FDODSC `0C 76 D0 · ★ **DBCLOB 입력 = 0xCF(NLOBCMIXED) · UTF-8**(서버가 413 DBCLOB CCSID 1200으로 변환 · 자리표시 길이 = UTF-8 바이트 · 0xCD 입력은 재판정 안 함) · **XML 입력 = SQL 재작성 `XMLPARSE(DOCUMENT CAST(? AS CLOB(2G)))`**(매개변수 989/LENGTH 0/CCSID 0 → 409 CLOB 0x7FFFFFFF CCSID 1208 · CLOB 경로 재사용) · 읽기 = DBCLOB QRYDSC `CD 8009`(EXTDTA = UTF-16BE 복원) · XMLSERIALIZE → `CF 8009` · 확정 = db2.yml 40183700337(3d3482ad) `db2_lob_dbclob_round_trip` · `db2_lob_xml_round_trip` 내장·확장 ✓(중간 회차 40182063764·40182500195·40183010063·40183353540의 "lob lengths don't match"/DTAMCHRM은 아래 FDODTA 행 지시자 버그 때문 · 0xCD·문자 수 가설 판정은 무효) | 03 0004 | CF 8009 | C9 8009` + `06 71 E4 D0 0001` · FDODTA LOB 자리 = `00 02` + 길이 u64 큰 끝 · NULL = `FF`만) → NULL 아닌 LOB마다 EXTDTA 0x146C 객체 DSS(본문 = `00` + 바이트 · CLOB = UTF-8 · DBCLOB 0xCD = UTF-16BE) · 32 767 B 초과 = DSS 길이 `FFFF` + 2 B 이어짐 머리 · DDM 확장 길이 `80 08` + u32(4 B 머리 제외 본문 길이 · 우리 해독기의 0x8004 가정 정정 = `ddm_len − 0x8004` = 확장 바이트 수) · 실측 = 312 193 B 체인 `FFFF D0 53 0001 | 8008 146C 0001…` → SQLCODE 0 · 1행 | 40162184058(ecc82c5b·7e21a90e) · 조사 drda-e(derby `buildPlaceholderLength`·`finalizeDssLength`·`updateLengthBytes` · jb `sql_dta.rs`·`dss.rs`) |
| ACCSECRD(SERVER_ENCRYPT) | ACCSEC(9 + 공개키 32 B) 한 번에 `0006 11A2 0009` + `0024 11DC <서버 공개키 32 B>` + **`0006 1909 0001`(ENCALG = 1 · DES)** → SECCHK(9 · SECTKN 둘) → SECCHKCD 0 + ACCRDBRM · 기본 AUTHENTICATION=SERVER에서는 {3, 5} + SECCHKCD 1(9 미지원) | 40162184058 · 40149595519 |
| SERVER_ENCRYPT 전환(CI) | `su - db2inst1 -c "db2 update dbm cfg using AUTHENTICATION SERVER_ENCRYPT ALTERNATE_AUTH_ENC NOT_SPECIFIED"` → `db2 terminate; db2stop force; db2start`(재시작 2 s) → `db2 connect to nexa` 첫 시도 성공 · 단계 13 s · cfg = SRVCON_AUTH NOT_SPECIFIED · TRUST_CLNTAUTH CLIENT · DB2AUTH 없음(STRICT_FIPS 아님) · ALTERNATE_AUTH_ENC NOT_SPECIFIED = 클라이언트 제안(DES) 수용 · 기존 5종 뒤 마지막 단계(평문 3이 거절될 수 있어 · 거절 여부 자체는 미확인) | 40162184058 · 조사 drda-f(IBM r0053734 · c0005435) |


#### 진단 손잡이(10-25)

| 환경 변수 | 값 | 동작 |
|---|---|---|
| `NSQL_TRACE_DRDA` | `1` | 송수신 DSS hexdump · 메시지당 512 B · 전체 4000줄 상한 · **SECCHK 체인은 통째 가림**("credentials masked") |
| | 숫자(예 `4096`) | 메시지당 그 바이트까지 hexdump(줄 상한·가림은 같음 · db2.yml = 4096) |
| `NSQL_DB2_AUTH` | (없음 = 기본) | 자동 = ACCSEC에 9(EUSRIDPWD) 제안 → 서버 목록에 9 없으면 3(USRIDPWD)으로 재협상 |
| | `plain` | 3만 제안(평문 · SERVER_ENCRYPT 서버면 거절 = ACCSECRD {7, 9, 10} + SECCHKCD 1) |
| | `encrypted` | 9만 허용 · 서버가 9를 안 주면 오류(3 폴백 없음) |

> ★ **보안 메모(10-25)** — SECMEC 9(EUSRIDPWD)는 DH 256비트 + **DES 56비트**라 오늘 기준으로 약하다 · IBM은 `SERVER_ENCRYPT`를 폐지 예고(지원 공지 node/7066501 · 12.1 문서 "deprecated") → **운영 권고 = TLS(`?tls=required`)** · 9는 SERVER_ENCRYPT 서버와의 호환용으로만 둔다.

남은 추정(2차 이후 · 10-25 저녁 갱신) = ~~SECMEC 9 실서버(서버 `SERVER_ENCRYPT`) · LOB 입력 EXTDTA~~(위 표로 확정) · ~~SERVER_ENCRYPT에서 평문 3 거절 여부~~ = SERVER_ENCRYPT에서 평문 3 = **거절**(ACCSEC 3 → ACCSECRD `000A 11A2 0007 0009 000A` = SECMEC {7, 9, 10} + `0005 11A4 01`(SECCHKCD 1 = 요청 방식 미지원) + `0008 1909 0001 0002` = ENCALG {1 DES, 2 AES} · 자격 증명 보내기 전 ACCSEC 단계에서 끝남 · db2.yml 40167412919 정보성 단계) · 다중 커서 섹션 · UTF-8 DB 문자열 타입 코드(0x30 vs 0x3C) 확인 · INTRDBRQS(쓰지 않음 · 취소 = 소켓 끊기로 확정). ~~취소 · EXTDTA LOB 조회 · CALL 결과 집합 · 라벨 열~~ = 위 표로 확정.

## 결론(5줄)

1. **프로토콜 = 단순 · 1차 최소 집합 작음** — 6바이트 DSS 머리(길이 2B · `0xD0` · 형식 바이트 · 상관 ID 2B) + DDM 객체(길이 2B · 코드포인트 2B · 하위 파라미터) · 틀은 큰 끝 · 1차 = 명령 ≈14 · 응답 메시지(RM) ≈20 · 객체 ≈10 · 흐름 = 접속 `EXCSAT → ACCSEC → SECCHK → ACCRDB` · DML/DDL `EXCSQLIMM+SQLSTT` · 질의 `PRPSQLSTT+SQLSTT+OPNQRY` 한 체인 → `SQLDARD · OPNQRYRM · QRYDSC · QRYDTA` → `CNTQRY` → `ENDQRYRM+SQLCARD` · `?` 바인드 = `SQLDTA(FDODSC+FDODTA)` · `RDBCMM`/`RDBRLLBCK` · 자동 커밋 = 클라이언트가 체인 끝에 RDBCMM.
2. **FD:OCA 1차 타입 = INTEGER · SMALLINT · BIGINT · FLOAT4/8 · DECIMAL(packed BCD) · CHAR/VARCHAR/LONG VARCHAR(+MIX) · DATE · TIME · TIMESTAMP · BOOLEAN** · 널 허용 = 홀수 코드 · 지시자 `0x00`/`0xFF` · ★ **SQL 데이터 정수·실수 바이트 순서 = 서버 TYPDEFNAM**(x86 Db2 LUW = `QTDSQLX86` 작은 끝 → ACCRDBRM에서 읽고 디코더 선택) · DDM 문자 파라미터 = EBCDIC 500 · SQL 문자 = TYPDEFOVR 1208(UTF-8)/1200 · 2차 = LOB(EXTDTA) · DECFLOAT · XML · GRAPHIC.
3. **보안 기본값** — Db2 LUW 기본 `AUTHENTICATION=SERVER` = **평문 SECMEC 3(USRIDPWD) 접속됨** · 1차 = SECMEC 3 + 9(EUSRIDPWD = 익명 DH 256비트 + DES/CBC/PKCS5 · p·g 상수와 키/IV 유도 = Derby·pydrda 동일 · IBM JCC 4.33+ 기본 9) · 실제 보호 = **TLS**(관례 50001 · TLS 먼저 맺고 그 위 DRDA · 이미 들어와 있는 rustls 0.23 + aws-lc-rs = D-348 재사용) · SERVER_ENCRYPT여도 평문 3이 통과(막으려면 서버 `DB2AUTH=JCC_ENFORCE_SECMEC`) · AES 변형 = 추정 · DES = 직접 구현 또는 `des` crate를 DR-3 예외 등재.
4. **CI 컨테이너 기본 인증 = SERVER(추정 · db2.yml이 별도 설정 없이 비밀번호 접속) · TLS 꺼짐(추정)** · Db2 LUW 고유 = 포트 50000 · RDBNAM 18바이트 공백 채움 · 동적 패키지 `NULLID.SYSSH200` · 섹션 65 · 일관성 토큰 `SYSLVL01`(ASCII) · QRYBLKSZ 65535(pydrda) · TIMESTAMP p 0..12(기본 6) · `SET CURRENT SCHEMA` = EXCSQLIMM · 취소 = INTRDBRQS(새 연결 · RDBINTTKN · Db2 LUW 지원 = 추정) → 대안 `ADMIN_CMD('FORCE APPLICATION (h)')`/`WLM_CANCEL_ACTIVITY`(h = 접속 직후 `MON_GET_APPLICATION_HANDLE()`).
5. **참고 구현체 · 범위** — 순수 Rust 선례 = crates.io **`justybase-db2`**(MIT · 0.1.0-alpha.3 · 2026-10-24 · ≈2.6만 줄 · go-db2 기반 · 의존 tokio·tokio-rustls·des·ring·crypto-bigint) · 순수 Go **`go-db2`**(MIT · cgo 없음 · ≈1.04만 줄 · SECMEC 3·9·TLS·LOB·CALL·취소) · **pydrda**(MIT · 2.3천 줄 · 8월 Derby 제거 = Db2 전용) · Derby 클라이언트(Apache-2.0 · 흐름 교과서 · 2025-10 은퇴) · IBM JCC = 독점(코드 열지 않음 · 결과 대조만) · 1차(pydrda 범위) ≈ Rust 4~6천 줄 · 2차(go-db2 수준 · LOB·CALL·취소·XML·DECFLOAT) ≈ 4~6천 줄 더 · 시험 = 캡처 DSS 바이트 오프라인 재생 + CI Db2 Community 컨테이너(Derby 서버는 CI 대상 아님).

---

> **결론**
> 1. DRDA는 **6바이트 DSS 머리 + DDM 객체(LL 2B · CP 2B · 하위 파라미터 LL+CP+데이터)** 의 단순한 큰 끝(big-endian) TLV 계층이다. 1단계 클라이언트에 필요한 코드포인트는 **명령 약 14개 · 응답 메시지(RM) 약 20개 · 객체 약 10개**로 작다.
> 2. 접속 = `EXCSAT → ACCSEC → SECCHK → ACCRDB`(SECMEC 3 = 평문 USRIDPWD로 시작 · 9 = DH+DES 암호화는 2차) · 문장 = `EXCSQLIMM+SQLSTT`(DML/DDL) · 질의 = `PRPSQLSTT+SQLSTT+OPNQRY` 한 체인 → `SQLDARD · OPNQRYRM · QRYDSC · QRYDTA … ENDQRYRM+SQLCARD` · 바인드 = `EXCSQLSTT/OPNQRY + SQLDTA(FDODSC+FDODTA)` · 트랜잭션 = `RDBCMM`/`RDBRLLBCK`(자동 커밋 = 클라이언트가 RDBCMM을 체인).
> 3. ★ **DDM 틀(길이·코드포인트)은 늘 큰 끝이지만, SQL 데이터(SQLCARD·SQLDARD·QRYDTA·SQLDTA의 정수/실수)의 바이트 순서는 `TYPDEFNAM`이 정한다** — Db2 LUW(x86)는 `QTDSQLX86`(작은 끝)으로 답하므로 ACCRDBRM의 TYPDEFNAM을 읽어 디코더를 고른다. 문자 = DDM 파라미터는 EBCDIC CCSID 500 · SQL 데이터는 `TYPDEFOVR`로 UTF-8(1208)/UTF-16(1200) 지정.
> 4. FD:OCA 1단계 타입 = INTEGER/SMALLINT/BIGINT/FLOAT4/FLOAT8/DECIMAL(packed BCD)/CHAR/VARCHAR/LONG VARCHAR/MIX·VARMIX/DATE/TIME/TIMESTAMP/BOOLEAN(+ 각 널 허용 홀수 코드) — LOB·DECFLOAT·XML·GRAPHIC은 2단계.

## ① DRDA 프로토콜 · FD:OCA

### 원천
| 원천 | URL | 무엇 |
|---|---|---|
| DRDA V5 Vol.1 (C112) | https://publications.opengroup.org/c112 | 흐름·명령 순서·TYPDEFNAM·SQLCARD/SQLDARD 정의 |
| DRDA V5 Vol.2 FD:OCA (C113) | https://publications.opengroup.org/c113 | 트리플릿(GDA·RLO·MDD·CPT)·데이터 타입 |
| DRDA V5 Vol.3 DDM (C114) | https://publications.opengroup.org/c114 | DSS·코드포인트 사전(EXCSAT·ACCSEC…) |
| Open Group 구판 목차 | https://pubs.opengroup.org/onlinepubs/9691989599/toc.pdf · https://pubs.opengroup.org/onlinepubs/009618799/toc.pdf | 공개 열람판(구판 C812/C814 계열) |
| Derby 클라이언트 | https://github.com/apache/derby/blob/trunk/java/org.apache.derby.client/org/apache/derby/client/net/CodePoint.java · `.../net/FdocaConstants.java` · `.../net/NetCursor.java` · `Request.java`/`Reply.java` | 코드포인트·FD:OCA 상수 |
| Derby 공통 | https://github.com/apache/derby/blob/trunk/java/org.apache.derby.commons/org/apache/derby/shared/common/reference/DRDAConstants.java | DRDA 타입 코드 전표 |
| pydrda (MIT) | https://github.com/nakagami/pydrda — `drda/ddm.py` · `connection.py` · `utils.py` · `codepoint.py` · `consts.py` | **실제 Db2 LUW 상대** 최소 구현(가장 실용적) |
| Wireshark 해부기 | https://gitlab.com/wireshark/wireshark/-/blob/master/epan/dissectors/packet-drda.c | DSS 플래그 비트·코드포인트 이름 |

※ Open Group 판 PDF는 무료 등록 후 내려받기(직접 링크 미확인 — 추정).

### 1-1. DSS 머리 (6 B · 큰 끝)
| 오프셋 | 크기 | 내용 |
|---|---|---|
| 0 | 2 | DSS 길이(머리 포함 · 최대 0x7FFF). **0xFFFF/최상위 비트 = 뒤에 이어짐(continuation)** — 이어지는 조각은 2B 길이 머리(0x7FFE = 또 이어짐) |
| 2 | 1 | 매직 `0xD0` |
| 3 | 1 | 형식 바이트: `0x40` 체인(뒤에 DSS 더 있음) · `0x20` 오류 시 계속(continue-on-error) · `0x10` 다음 DSS가 **같은 상관 ID** · 하위 4비트 = 종류 `1` RQSDSS(요청) · `2` RPYDSS(응답) · `3` OBJDSS(객체) · `4` CMNDSS(통신) · `5` RQSDSS 응답 없음(추정) |
| 4 | 2 | 상관 ID(correlation id) — 요청마다 1씩 증가 · 같은 명령에 딸린 객체(SQLSTT·SQLDTA)는 같은 ID |

pydrda 실제 값: `0x01` 요청 · `0x03` 객체 · `0x41` 체인 요청(다음 ID 증가) · `0x51` 체인 요청 + 다음 같은 ID(=명령 뒤에 SQLSTT 객체) · `0x43` 체인 객체. 객체(OBJDSS)로 보내는 것 = SQLSTT · SQLATTR · SQLDTA · EXTDTA.

**DDM 객체** = `LL(2B, 자신 포함) · CP(2B) · 본문`. 본문 = 하위 파라미터 `LL(2) CP(2) 데이터` 나열. **확장 길이**: LL ≥ 0x8000이면 `LL & 0x7FFF` = 뒤따르는 길이 바이트 수 → `0x8004`(4B 길이) · `0x8008`(8B) · 큰 QRYDTA/EXTDTA에서 씀(pydrda가 QRYDTA `0x8004` 처리).
**체인 규칙**: 한 번에 여러 명령을 체인으로 보내고(마지막만 체인 비트 0), 서버는 각 명령에 대해 응답을 순서대로 체인해 돌려준다. 오류 RM이 오면 그 체인의 나머지는 처리되지 않을 수 있다(continue-on-error 비트 미설정 시 · 추정).

### 1-2. 코드포인트 표 (최소 클라이언트용 · 출처 = Derby CodePoint.java / pydrda codepoint.py)
| 분류 | 이름 | CP | 용도 |
|---|---|---|---|
| 명령 | EXCSAT / EXCSATRD | 0x1041 / 0x1443 | 서버 속성 교환 / 응답 |
| 〃 | ACCSEC / ACCSECRD | 0x106D / 0x14AC | 보안 방식 협상 / 응답(SECMEC·SECTKN) |
| 〃 | SECCHK / SECCHKRM | 0x106E / 0x1219 | 인증 / 결과(SECCHKCD 0x11A4 · 0 = 성공) |
| 〃 | ACCRDB / ACCRDBRM | 0x2001 / 0x2201 | DB 접근 / 응답(서버 TYPDEFNAM·PRDID·CRRTKN) |
| 〃 | PRPSQLSTT | 0x200D | 준비(RTNSQLDA 0x2116 = 0xF1 → SQLDARD 받음) |
| 〃 | DSCSQLSTT | 0x2008 | 기술(TYPSQLDA 0x2146 · 입력 파라미터 기술 = 1) |
| 〃 | EXCSQLIMM | 0x200A | 즉시 실행(DML/DDL) |
| 〃 | EXCSQLSTT | 0x200B | 준비문 실행(+SQLDTA) |
| 〃 | EXCSQLSET | 0x2014 | SET 문(특수 레지스터) |
| 〃 | OPNQRY / CNTQRY / CLSQRY | 0x200C / 0x2006 / 0x2005 | 커서 열기 / 다음 블록 / 닫기 |
| 〃 | RDBCMM / RDBRLLBCK | 0x200E / 0x200F | 커밋 / 롤백(본문 없음) |
| EXCSAT 파라미터 | EXTNAM · SRVNAM · SRVCLSNM · SRVRLSLV · MGRLVLLS | 0x115E · 0x116D · 0x1147 · 0x115A · 0x1404 | 외부 이름 · 서버 이름 · 제품 등급 · 릴리스 · 관리자 수준 목록 |
| 관리자 | AGENT · SQLAM · RDB · SECMGR · CMNTCPIP · CCSIDMGR · UNICODEMGR | 0x1403 · 0x2407 · 0x240F · 0x1440 · 0x1474 · 0x14CC · 0x1C08 | MGRLVLLS 안 `CP(2)+수준(2)` 쌍 |
| 보안 | SECMEC · SECTKN · USRID · PASSWORD | 0x11A2 · 0x11DC · 0x11A0 · 0x11A1 | 방식(2B) · 토큰(DH 공개키·암호문) · 사용자 · 암호 |
| ACCRDB 파라미터 | RDBNAM · RDBACCCL · PRDID · TYPDEFNAM · TYPDEFOVR · CRRTKN | 0x2110 · 0x210F · 0x112E · 0x002F · 0x0035 · 0x2135 | DB 이름 · 접근 관리자(=SQLAM) · 제품 ID · 데이터 표현 · CCSID 덮어쓰기 · 상관 토큰 |
| CCSID | CCSIDSBC · CCSIDDBC · CCSIDMBC | 0x119C · 0x119D · 0x119E | TYPDEFOVR 하위(2B CCSID) |
| 문장 파라미터 | PKGNAMCSN · PKGSN · RDBCMTOK · QRYBLKSZ · MAXBLKEXT · QRYROWSET · QRYINSID · QRYCLSIMP · DYNDTAFMT · RTNEXTDTA · OUTEXP · NBRROW · QRYPRCTYP · UOWDSP | 0x2113 · 0x210C · 0x2105 · 0x2114 · 0x2141 · 0x2156 · 0x215B · 0x215D · 0x214B · 0x2148 · 0x2111 · 0x213A · 0x2102 · 0x2115 | 패키지 이름+일관성 토큰+섹션 · 커밋 허용 · 블록 크기(4B) · 추가 블록 수 · 행 묶음 · 질의 인스턴스 ID(8B) · 마지막 행 뒤 자동 닫기 · 동적 데이터 형식 · EXTDTA 반환 방식 · … · UOW 처분 |
| 객체 | SQLSTT · SQLATTR · SQLDTA · FDODSC · FDODTA · SQLCARD · SQLDARD · SQLCINRD · QRYDSC · QRYDTA · SQLDTARD · EXTDTA | 0x2414 · 0x2450 · 0x2412 · 0x0010 · 0x147A · 0x2408 · 0x2411 · 0x240B · 0x241A · 0x241B · 0x2413 · 0x146C | 문장 · 커서 속성 · 입력 데이터 · 서술자 · 데이터 · 통신 영역 · 서술자 영역 · 결과셋 열 정보 · 질의 서술자 · 행 블록 · · LOB 외부 데이터 |
| 응답 메시지(RM) | SVRCOD(공통 · 0x1149) · OPNQRYRM 0x2205 · ENDQRYRM 0x220B · ENDUOWRM 0x220C · RDBUPDRM 0x2218 · SQLERRRM 0x2213 · RDBACCRM 0x2207 · RDBNFNRM 0x2211 · RDBAFLRM 0x221A · RDBNACRM 0x2204 · QRYNOPRM 0x2202 · QRYPOPRM 0x220F · OPNQFLRM 0x2212 · DTAMCHRM 0x220E · RSLSETRM 0x2219 · ABNUOWRM 0x220D · CMDATHRM 0x121C | | 성공·상태·오류 보고 |
| 형식 오류 RM | SYNTAXRM 0x124C(+SYNERRCD 0x114A) · PRCCNVRM 0x1245(+PRCCNVCD 0x113F) · AGNPRMRM 0x1232 · CMDNSPRM 0x1250 · PRMNSPRM 0x1251 · VALNSPRM 0x1252 · OBJNSPRM 0x1253 · CMDCHKRM 0x1254 · MGRLVLRM 0x1210 · RSCLMTRM 0x1233 | | 프로토콜 위반(=클라이언트 버그) · CODPNT 0x000C · SRVDGN 0x1153 동반 |

SVRCOD(2B): 0 정보 · 4 경고 · 8 오류 · 16 심각 · 32 접근 손상 · 64 영구 손상 · 128 세션 손상(≥8 = 실패 처리).

### 1-3. 흐름

**접속** (pydrda `connection.py` 기준)
1. `EXCSAT`(상관 1): EXTNAM·SRVNAM·SRVRLSLV·SRVCLSNM = **EBCDIC cp500** · MGRLVLLS = `AGENT 10 · SQLAM 11 · CMNTCPIP 5 · RDB 12 · SECMGR 9 · UNICODEMGR 1208`(Derby 클라이언트는 7 계열 · Db2는 받은 목록 중 지원 수준으로 낮춰 응답). → `EXCSATRD`(서버 수준·SRVRLSLV 예 `SQL11059`).
2. `ACCSEC`: SECMEC(2B) + RDBNAM(cp500) [+ SECTKN = 클라이언트 DH 공개키 32B(SECMEC 9)]. → `ACCSECRD`: 서버가 지원하는 SECMEC(다르면 그 값으로 ACCSEC 재시도) + 서버 SECTKN.
3. `SECCHK`: SECMEC + RDBNAM + USRID + PASSWORD(SECMEC 3 평문) 또는 SECTKN×2(SECMEC 9 = DH 공유키로 DES 암호화한 사용자·암호). → `SECCHKRM`(SVRCOD + SECCHKCD; 0 = 성공).
4. `ACCRDB`: RDBNAM · RDBACCCL=SQLAM(0x2407) · PRDID(예 `SQL11010`/`JCC04200` 꼴 8자) · TYPDEFNAM=`QTDSQLX86` · TYPDEFOVR=`0006 119C 04B8 | 0006 119D 04B0 | 0006 119E 04B8`(SBC 1208 · DBC 1200 · MBC 1208) · CRRTKN(임의 고유 토큰). → `ACCRDBRM`(서버 TYPDEFNAM·TYPDEFOVR·PRDID) + 대개 SQLCARD.
5. (선택) `EXCSAT(MGRLVLLS = CCSIDMGR 1208)` → `EXCSQLSET` + `SQLSTT "SET CLIENT …"` → `RDBCMM`.
SECMEC 값(pydrda consts): 3 USRIDPWD · 4 USRIDONL · 7 USRENCPWD · 8 USRSSBPWD · 9 EUSRIDPWD. Db2 기본 `AUTHENTICATION=SERVER`는 3 수락 · `SERVER_ENCRYPT`면 9 필요.

**즉시 실행(DML/DDL)**: `EXCSQLIMM(PKGNAMCSN, RDBCMTOK=0xF1)` 체인 `0x51` + `SQLSTT`(OBJDSS 같은 ID) → `SQLCARD` (+ 갱신 시 `RDBUPDRM`) · 오류면 `SQLCARD`(SQLCODE<0) 또는 `SQLERRRM`+SQLCARD. 영향 행 수 = SQLCARD의 SQLERRD[2](3번째 · 추정).
**질의**: `PRPSQLSTT(PKGNAMCSN, RTNSQLDA=0xF1)` + `SQLSTT` + `OPNQRY(PKGNAMCSN, QRYBLKSZ=65535(4B), MAXBLKEXT(2B), QRYCLSIMP=0x01)` 한 체인 → `SQLDARD`(열 메타) · `OPNQRYRM`(QRYINSID 등) · `QRYDSC` · `QRYDTA`(+ LOB면 `EXTDTA`) … 마지막 블록이면 `ENDQRYRM` + `SQLCARD`(SQLCODE 100). 남으면 `CNTQRY(PKGNAMCSN, QRYBLKSZ, QRYINSID, RTNEXTDTA=0x02)` 반복 · 중도 포기 = `CLSQRY`.
**파라미터 바인드**: `PRPSQLSTT + SQLSTT + DSCSQLSTT(TYPSQLDA=1)` → SQLDARD 두 개(첫 바이트 0xFF로 시작하는 쪽 = 입력 파라미터 기술 · pydrda) → `EXCSQLSTT(+RDBCMTOK) + SQLDTA` 또는 `OPNQRY(+DYNDTAFMT=0xF1) + SQLDTA`.
**PKGNAMCSN** (고정 64B): RDBNAM 18 · RDBCOLID 18(`NULLID`) · PKGID 18(`SYSSH200`) · 일관성 토큰 8(`SYSLVL01`) · PKGSN 2(섹션 · pydrda 기본 65). 모두 공백 채움 · Db2 LUW 기본 바인드된 CLI 패키지 사용 · **동시에 열린 커서마다 다른 섹션 번호**(추정).
**커밋/롤백**: `RDBCMM`/`RDBRLLBCK`(본문 없음) → `ENDUOWRM(UOWDSP)` + `SQLCARD`. 자동 커밋 = 문장 체인 끝에 RDBCMM을 붙이는 클라이언트 방식(JCC·Derby 동일 · 추정) · 서버측 자동 커밋 플래그 없음 · RDBCMTOK는 "문장 안 COMMIT 허용" 뜻. 닫기 = RDBRLLBCK 후 소켓 종료.
**오류**: SQLCARD SQLCODE<0 → 오류(SQLSTATE 5자 · SQLERRMC 토큰 = 0xFF 구분 · pydrda). 체인 응답은 끝(마지막 DSS 체인 비트 0)까지 다 읽어 흐름을 맞춘 뒤 오류를 올린다.

### 1-4. FD:OCA · 데이터
**TYPDEFNAM**: `QTDSQL370`(IBM 370 · EBCDIC · 큰 끝 · S/390 실수) · `QTDSQL400` · `QTDSQLX86`(작은 끝 정수 · IEEE 실수 · 작은 끝) · `QTDSQLASC`(큰 끝 · IEEE · ASCII) · `QTDSQLVAX`. 요청자가 보내는 데이터 = 요청자 TYPDEFNAM · 서버 응답 데이터 = ACCRDBRM의 서버 TYPDEFNAM(Db2 LUW x86 = X86). **DDM LL·CP와 FD:OCA 길이 앞머리(VARCHAR 2B 등)·CCSID 필드는 늘 큰 끝**(pydrda utils · SQLDARD CCSID).
**CCSID**: DDM 문자 파라미터(EXTNAM·SRVNAM·RDBNAM·USRID·PASSWORD·PRDID·TYPDEFNAM·PKGNAMCSN) = CCSID 500 EBCDIC(pydrda는 일부를 `enc`로) · SQL 문장/데이터 = TYPDEFOVR 1208(UTF-8) · GRAPHIC/DBCS = 1200(UTF-16BE).

**QRYDSC**(질의 서술자 · Vol.2 트리플릿): `LL 76 D0 [타입 1B · 길이 2B]×열` = **NGDA(0x76) 그룹 D0**(SQLDTAGRP) → `06 71 E4 D0 00 01`(RLO · SQLDTA 행 = D0 1회) → `06 71 F0 E0 00 00`(RLO · SQLDTARD = SQLCADTA 반복) · MDD = `07 78 00 05 …`(SQLDTAGRP `02 01 D0` · SQLDTA `03 01 E4`) · 한 NGDA 최대 84열(넘으면 여러 트리플릿) · CPT(0x7F)는 이어짐. 길이 2B: DECIMAL = 정밀도(1B)·스케일(1B) · LOB = `0x8000|길이바이트수`.
**QRYDTA 행**: 행마다 `SQLCAGRP 지시자`(0xFF = SQLCA 없음 = 정상 행 · 0x00 = SQLCARD 따라옴 → 끝/경고) + `행 그룹 지시자`(0x00) + 열 값들(널 허용 열 = 1B 지시자 0x00 값/0xFF NULL → 값 생략). 블록 경계에서 행이 잘릴 수 있다(다음 QRYDTA로 이어 붙임 · 추정).
**SQLCARD**(Vol.1 SQLCAGRP): 지시자 1B(0xFF = 정상·없음) · SQLCODE 4B(TYPDEF 순서) · SQLSTATE 5B · SQLERRPROC 8B · SQLCAXGRP 지시자 · SQLERRD 6×4B · SQLWARN 11B · RDBNAM(VCS 2B길이) · SQLERRMSG_m/_s(VCM/VCS) · SQLDIAGGRP 지시자(추정 세부).
**SQLDARD**: SQLCARD + SQLDHGRP(지시자 …) + 열 수 2B + 열마다 `SQLDAGRP` = PRECISION 2 · SCALE 2 · LENGTH 8 · SQLTYPE 2(TYPDEF 순서) · CCSID 2(큰 끝) · SQLDOPTGRP(지시자 · UNNAMED 2 · NAME_m/_s · LABEL_m/_s · COMMENTS_m/_s · UDT·DX 그룹 지시자) — `_m`/`_s` 각각 2B 길이 + 바이트(혼합/단일 문자) · SQLTYPE은 Db2 SQL 타입(INTEGER 496/497…).

**타입 표**(DRDA 타입 = Derby DRDAConstants · 길이 = pydrda utils/ddm)
| SQL 타입 | DRDA(널X/널O) | Db2 SQLTYPE | 선 위 형식 |
|---|---|---|---|
| SMALLINT | 0x04/0x05 | 500/501 | 2B 정수(TYPDEF 순서) |
| INTEGER | 0x02/0x03 | 496/497 | 4B 정수 |
| BIGINT | 0x16/0x17 | 492/493 | 8B 정수 |
| REAL | 0x0C/0x0D | 480/481(len 4) | 4B IEEE(X86 = 작은 끝) |
| DOUBLE | 0x0A/0x0B | 480/481(len 8) | 8B IEEE |
| DECIMAL(p,s) | 0x0E/0x0F | 484/485 | packed BCD `p/2+1` B · 마지막 니블 부호 C/F=+ · D=− · 길이 필드 = p·s 바이트 |
| NUMERIC(zoned) | 0x10/0x11 | 488/489 · 504/505 | zoned 십진(Db2 LUW는 DECIMAL로 옴 · 추정) |
| CHAR(n) | 0x30/0x31 | 452/453 | 고정 n B (CCSIDSBC) |
| VARCHAR | 0x32/0x33 | 448/449 | 2B 길이(큰 끝) + 바이트 |
| LONG VARCHAR | 0x34/0x35 | 456/457 | 2B 길이 + 바이트 |
| CHAR/VARCHAR(혼합·UTF-8) | 0x3C/0x3D · 0x3E/0x3F · 0x40/0x41 | 동일 | MIX 고정 / VARMIX 2B길이 / LONGMIX (CCSIDMBC 1208) — **Db2 LUW UTF-8 DB의 일반 문자열은 대개 MIX 계열(추정)** |
| GRAPHIC/VARGRAPHIC | 0x36/0x37 · 0x38/0x39 · 0x3A/0x3B | 468/469 · 464/465 | UTF-16BE(CCSIDDBC 1200) · 길이 = 2바이트 문자 수(추정) |
| BINARY/VARBINARY | 0x26/0x27 · 0x28/0x29 | 912/913 · 908/909 | 고정 / 2B 길이 |
| DATE | 0x20/0x21 | 384/385 | 10B 문자 `YYYY-MM-DD` |
| TIME | 0x22/0x23 | 388/389 | 8B `HH.MM.SS`(또는 `HH:MM:SS`) |
| TIMESTAMP | 0x24/0x25 | 392/393 | 26B `YYYY-MM-DD-HH.MM.SS.ffffff`(정밀도 12 = 32B · 보낼 때 pydrda 32B) |
| BOOLEAN | 0xBE/0xBF | 2436/2437 | 1B |
| DECFLOAT(16/34) | 0xBA/0xBB(pydrda) | 996/997 | 8B/16B IEEE 754 DPD |
| BLOB / CLOB / DBCLOB | 0xC8/0xC9 · 0xCA/0xCB(SBCS) · 0xCC/0xCD(DBCS) · 0xCE/0xCF(MIXED) | 404 · 408 · 412 | 행 안 = 상태·길이 자리표시 · 실제 = **EXTDTA 0x146C**(확장 길이) · 또는 로케이터 0x18~0x1D(4B) |
| XML | (VARMIX/LOB로 투영) | 988/989 | 추정 |

**SQLDTA**(입력): `FDODSC` = `LL 76 D0 [타입 길이]×n` + `06 71 E4 D0 00 01` + MDD; `FDODTA` = (홀수 길이면 0x00 패드 · pydrda) + 값마다 널 허용 지시자(0x00/0xFF) + 값. pydrda는 문자열을 `NVARCHAR 39 3F FF`(VARGRAPHIC · UTF-16BE)로 보낸다 — 단순화를 위해 **모든 입력을 널 허용 타입으로 기술**하는 방식이 실용적.

### 1-5. 1단계 최소 집합
- **명령**: EXCSAT · ACCSEC · SECCHK · ACCRDB · EXCSQLIMM · PRPSQLSTT · DSCSQLSTT · EXCSQLSTT · OPNQRY · CNTQRY · CLSQRY · RDBCMM · RDBRLLBCK (+ EXCSQLSET 선택)
- **보내는 파라미터/객체**: EXTNAM · SRVNAM · SRVCLSNM · SRVRLSLV · MGRLVLLS(AGENT·SQLAM·RDB·SECMGR·CMNTCPIP·UNICODEMGR/CCSIDMGR) · SECMEC · SECTKN · USRID · PASSWORD · RDBNAM · RDBACCCL · PRDID · TYPDEFNAM · TYPDEFOVR(CCSIDSBC/DBC/MBC) · CRRTKN · PKGNAMCSN · RDBCMTOK · RTNSQLDA · TYPSQLDA · QRYBLKSZ · MAXBLKEXT · QRYCLSIMP · QRYINSID · RTNEXTDTA · DYNDTAFMT · SQLSTT · SQLDTA(FDODSC·FDODTA)
- **받는 것**: EXCSATRD · ACCSECRD · SECCHKRM(SECCHKCD) · ACCRDBRM · SQLCARD · SQLDARD · OPNQRYRM · QRYDSC · QRYDTA · ENDQRYRM · ENDUOWRM · RDBUPDRM · SQLERRRM · 오류 RM 전부(SVRCOD·CODPNT·SRVDGN만 읽어 오류로 변환 — SYNTAXRM·PRCCNVRM·AGNPRMRM·CMDNSPRM·PRMNSPRM·VALNSPRM·OBJNSPRM·CMDCHKRM·MGRLVLRM·RDBNFNRM·RDBAFLRM·RDBNACRM·QRYNOPRM·QRYPOPRM·OPNQFLRM·DTAMCHRM·ABNUOWRM·CMDATHRM) · 모르는 CP = 건너뜀
- **FD:OCA 타입**: 0x02~0x05 · 0x16/17 · 0x0A~0x0D · 0x0E/0F · 0x30~0x35 · 0x3C~0x41 · 0x20~0x25 · 0xBE/BF (+ 0x26~0x29 바이너리 권장) · 트리플릿 NGDA 0x76 · RLO 0x71 · MDD 0x78 · CPT 0x7F
- **보안**: SECMEC 3(평문) 먼저 → 9(DH+DES, 256비트 DH 소수 · DES = 자체 구현 필요) 2단계

### 추정 항목
- DSS 종류 5(응답 없는 요청)의 정확한 의미 · continue-on-error 미설정 시 체인 중단 규칙.
- SQLCARD 세부 순서(SQLCAXGRP 안 필드 · SQLDIAGGRP) 및 영향 행 수 = SQLERRD[2] — Vol.1 SQLCAGRP 절로 재확인 필요.
- QRYDTA 행 앞 두 지시자의 정확한 의미(pydrda는 첫 바이트 0xFF만 검사) · 블록 경계에서 행 분할 처리.
- Db2 LUW UTF-8 DB에서 CHAR/VARCHAR가 0x30/0x32(SBCS)와 0x3C/0x3E(MIX) 중 무엇으로 오는지 — 실서버 덤프로 확인.
- 0xCE/0xCF 이름: Derby = LOBCMIXED · pydrda 주석 = LOBCSBCS(불일치 · Derby 따름). DECFLOAT 0xBA/0xBB는 Derby에 없음(pydrda만).
- 동시 커서마다 다른 PKGSN 필요 여부 · 자동 커밋을 클라이언트 RDBCMM 체인으로만 구현하는지(JCC 동작).
- Open Group C112/C113/C114 PDF 무료 내려받기 링크(등록 필요 여부) 미확인.
- SECMEC 9의 DH 소수·생성자 값과 DES 키 유도(pydrda `secmec9.py`에 있음 · 이번에 미열람).

---

조사일 2026-10-25 · 웹 자료만 봤다(저장소는 고치지 않음). 출처 확인이 안 된 항목은 "추정"으로 표시했다.

## ② 보안 메커니즘 · TLS

**결론**: Db2 LUW 기본값은 `AUTHENTICATION=SERVER`이고 이때 평문 USRIDPWD(3)가 받아들여진다. 이 저장소 CI도 `icr.io/db2_community/db2:12.1.2.0` 컨테이너에 아무 설정 없이 비밀번호로 접속하고 있다.
첫 구현은 **SECMEC 3 + 9(EUSRIDPWD, DH 256비트 + DES-CBC)** 로 충분하다. 9는 pydrda 기준 파이썬 60줄 정도이고, IBM JCC 4.33(11.5.9)부터 기본값이 9다. 운영 환경 보호는 DRDA 암호화보다 **TLS(SSL_SVCENAME, 보통 50001)** 로 한다. IBM도 DRDA 암호화가 익명 DH라서 MITM에 약하다고 적고 있다.

### SECMEC 코드 (ACCSEC/SECCHK)
| 코드 | 이름 | 내용 | Db2 LUW | 비고 |
|---|---|---|---|---|
| 3 | USRIDPWD | 사용자·비밀번호 평문 | ✅ (SERVER · SERVER_ENCRYPT · GSS_SERVER_ENCRYPT) | JCC `CLEAR_TEXT_PASSWORD_SECURITY` |
| 4 | USRIDONL | 사용자만 | CLIENT 인증일 때만(CLIENT는 11.5.9에서 폐지 예고) | |
| 5 | USRIDNWPWD | 사용자·비밀번호·새 비밀번호 | 비밀번호 변경용 | JCC `CHANGE_PASSWORD` 계열 — 추정 |
| 7 | USRENCPWD | 비밀번호만 암호화 | ✅ SERVER_ENCRYPT | IV = 사용자 ID 8바이트(0 채움 또는 자름) |
| 8 | USRSSBPWD | 강한 비밀번호 대체(SHA-1) | ❌ Derby 전용으로 보임(추정) | Derby `substitutePassword` |
| 9 | EUSRIDPWD | 사용자 ID와 비밀번호 모두 암호화 | ✅ SERVER_ENCRYPT(+SERVER에서도 협상됨 — 추정) | JCC 4.33부터 기본값 |
| 11 | KERSEC | Kerberos | KERBEROS · KRB_SERVER_ENCRYPT | 1차 범위 밖 |
| 12 / 13 | EUSRIDDTA / EUSRPWDDTA | 데이터까지 암호화 | DATA_ENCRYPT(폐지 예고) | 구현하지 않는다 |
| 15 | PLGIN | GSS 플러그인 | GSSPLUGIN | 범위 밖 |
| 16 / 18 | EUSRIDONL / TLS 클라이언트 인증서 | | | JCC 상수 기준 — 추정 |

- 협상 방식: ACCSEC에 원하는 SECMEC를 보낸다. 서버가 그 값을 지원하지 않으면 ACCSECRD에 **지원 목록**을 돌려준다. pydrda는 9로 시작해서 서버가 돌려준 값으로 ACCSEC를 다시 보낸다. Derby는 다시 시도하지 않고 오류로 끝낸다(`NetConnection.setAccessSecurityData`). JCC 문서에는 "3이 거부되고 9가 지원되면 9로 올려 재시도한다"고 되어 있다.
- 출처: Derby `NetConfiguration.java`(SECMEC_* 상수) https://github.com/apache/derby/blob/trunk/java/org.apache.derby.client/org/apache/derby/client/net/NetConfiguration.java · MS HIS DRDA 표 https://learn.microsoft.com/en-us/host-integration-server/core/protection2 · JCC 보안(LUW) https://www.ibm.com/support/knowledgecenter/SSEPGG_11.5.0/com.ibm.db2.luw.apdv.java.doc/src/tpc/imjcc_cjvjcsec.html · JCC 4.33 기본값 9 https://www.ibm.com/docs/en/SSEPGG_11.5.0/com.ibm.swg.im.dbclient.config.doc/doc/r_dsd-config_securitymechanism.html

### EUSRIDPWD(9) 알고리즘 — Derby와 pydrda가 같은 값을 쓴다
| 항목 | 값 |
|---|---|
| 소수 p (32바이트, 빅엔디언) | `C62112D73EE613F0947AB31F0F6846A1BFF5B3A4CA0D60BC1E4C7A0D8C16B3E3` |
| 생성원 g (32바이트) | `4690FA1F7B9E1D4442C86C9114603FDECF071EDCEC5F626E21E256AED9EA34E4` |
| 개인키 x | pydrda: `randrange(2, p)` · Derby: JCE DH, 지수 길이 255비트 |
| 클라이언트 공개키 | `g^x mod p`를 32바이트로 맞춘다(33바이트면 앞의 0을 떼고, 짧으면 앞을 0으로 채움). ACCSEC의 **SECTKN**으로 보낸다 |
| 서버 공개키 | ACCSECRD의 SECTKN(32바이트) |
| 공유 비밀 | `server_pub^x mod p`를 32바이트 빅엔디언으로 |
| DES 키 | **공유 비밀 [12..20)**, 가운데 8바이트. Derby는 홀수 패리티로 보정한다(바이트마다 1의 개수가 짝수면 최하위 비트를 뒤집음). pydrda는 보정하지 않는다(DES가 패리티 비트를 무시하므로 결과는 같다) |
| IV | **서버 공개키(SECTKN) [12..20)** — Derby는 `encryptData(..., initVector=targetPublicKey_, targetPublicKey_)`, pydrda는 `iv = server_sectkn[12:20]` |
| 암호 방식 | `DES/CBC/PKCS5Padding`. 사용자 ID와 비밀번호를 **각각 따로** 암호화해서 SECCHK의 SECTKN 두 개로 보낸다(사용자 → 비밀번호 순) |
| 평문 인코딩 | 접속 시 협상한 CCSID의 바이트(pydrda는 EBCDIC cp500 또는 UTF-8) — 정확한 순서·인코딩은 패킷 캡처로 확인 필요(추정) |

- USRENCPWD(7)는 키는 같고 IV만 **사용자 ID 8바이트**다(Derby `EncryptionManager`).
- USRSSBPWD(8, Derby): `t = SHA1(hexbytes(user) ‖ hexbytes("3b60"+hex(SHA1(hexbytes(pw)))))`, 그다음 `PW_SUB = SHA1(t ‖ 서버 시드 ‖ 클라이언트 시드 ‖ user ‖ 00..01)`. Db2 LUW에서는 쓰이지 않는다(추정).
- **AES 변형**: JCC `encryptionAlgorithm=2`(AES 256비트, 기본값 1 = DES 56비트), CLI는 `ClientEncAlg=2`. ACCSEC에 **ENCALG**(=AES)과 **ENCKEYLEN**을 덧붙인다. 코드포인트(ENCALG 0x1909 · ENCKEYLEN 0x190A)와 AES용 DH 소수 크기 · 키·IV 유도 방식(SHA-256 사용 여부)은 공개 자료에서 찾지 못했다(추정 — Open Group DRDA Vol.3 "Security" 장을 원문으로 확인할 것). 서버 쪽 `ALTERNATE_AUTH_ENC=AES_ONLY`인 곳에서만 꼭 필요하다.
- 출처: Derby `EncryptionManager.java` https://github.com/apache/derby/blob/trunk/java/org.apache.derby.client/org/apache/derby/client/am/EncryptionManager.java · Derby `NetConnection.java` (같은 디렉터리 `client/net/`) · pydrda `drda/secmec9.py`·`ddm.py`(packSECCHK) https://github.com/nakagami/pydrda/blob/master/drda/secmec9.py · JCC 암호화 https://www.ibm.com/docs/en/SSEPEK_12.0.0/java/src/tpc/imjcc_cjvjcsen.html

### 서버 DBM CFG `AUTHENTICATION` (기본값 = SERVER)
| 값 | 의미 | 받는 SECMEC (JCC 호환표 기준) |
|---|---|---|
| **SERVER** (기본) | 서버가 인증하고 암호화는 필수가 아님 | 3 (9도 받는다 — 추정: pydrda가 9로 시작해 기본 컨테이너에 붙는 근거) |
| SERVER_ENCRYPT (폐지 예고) | 자격 증명 암호화 필수 | 7 · 9 · 그리고 **3도 통과한다**(JCC 표. 막으려면 레지스트리 `DB2AUTH=JCC_ENFORCE_SECMEC`) |
| SERVER_ENCRYPT_AES (폐지 예고) | AES로 자격 증명 암호화 | 9 + ENCALG=AES |
| DATA_ENCRYPT / DATA_ENCRYPT_CMP (폐지 예고) | 데이터까지 DES로 암호화(_CMP는 옛 클라이언트 SERVER_ENCRYPT도 허용) | 13 (_CMP는 7·9도) |
| CLIENT (11.5.9에서 폐지 예고) | 클라이언트가 인증 | 4 |
| KERBEROS / KRB_SERVER_ENCRYPT | Kerberos (_ENCRYPT는 SERVER_ENCRYPT로 폴백) | 11 / 11 · 15 · 7 · 9 |
| GSSPLUGIN / GSS_SERVER_ENCRYPT | GSS 플러그인 (_ENCRYPT는 폴백) | 15 · 11 / 3 · 7 · 9 · 15 · 11 |

- `ALTERNATE_AUTH_ENC`(SERVER_ENCRYPT에만 적용): `NOT_SPECIFIED`(기본, 클라이언트가 제안한 알고리즘을 받음) · `AES_CMP`(AES·DES 모두 받고 가능하면 AES) · `AES_ONLY`(DES 클라이언트는 거부).
- 11.5와 12.1의 기본값은 모두 SERVER이고 평문을 거부하지 않는다. 바뀐 것은 JCC 클라이언트 기본값이 3에서 9로 옮겨 간 것(4.33)뿐이다. 12.1에서 서버 기본값이 바뀌었다는 자료는 찾지 못했다(추정: 변경 없음).
- 출처: https://www.ibm.com/docs/en/db2/11.5.x?topic=parameters-authentication-authentication-type · https://www.ibm.com/docs/en/SSHRBY/com.ibm.db2.luw.admin.config.doc/doc/r0053734.html

### TLS
| 항목 | 내용 |
|---|---|
| 서버 설정 | `SSL_SVCENAME`(기본 null) · `DB2COMM=SSL,TCPIP` · `SSL_SVR_KEYDB`/`SSL_SVR_STASH`/`SSL_SVR_LABEL`. TCPIP와 같은 포트는 쓸 수 없다. 관례상 **50001** |
| 프로토콜 | **TLS를 먼저 맺고 그 위에서 평범한 DRDA**(EXCSAT부터). STARTTLS 같은 단계는 없다(pydrda가 `wrap_socket` 뒤에 그대로 DRDA를 보냄) |
| 인증서 | 서버 인증서를 검증한다(Db2 on Cloud와 자체 서명 인증서는 CA 파일로). 클라이언트 인증서 인증 = JCC `TLS_CLIENT_CERTIFICATE_SECURITY`(범위 밖) |
| TLS 위의 인증 | 보통 SECMEC 3 (채널이 이미 암호화돼 있음) |
- 출처: https://www.ibm.com/docs/en/SSHRBY/com.ibm.db2.luw.admin.config.doc/doc/r0053615.html · pydrda `connection.py`

### Db2 Community 컨테이너 (`icr.io/db2_community/db2`)
- 환경변수: `LICENSE=accept` · `DB2INSTANCE` · `DB2INST1_PASSWORD` · `DBNAME` · `BLU` · `ENABLE_ORACLE_COMPATIBILITY` · `TO_CREATE_SAMPLEDB` · `UPDATEAVAIL` 등. **SSL 관련 변수는 없다** → TLS는 꺼진 상태로 뜬다(추정, 직접 켜야 함). 기동에 1~2분 걸린다.
- AUTHENTICATION: 이미지 기본값을 직접 확인한 자료는 없다. 인스턴스 기본값인 **SERVER로 보이며 평문 USRIDPWD로 바로 접속된다** — 이 저장소 `.github/workflows/db2.yml`(12.1.2.0, ODBC + `db2inst1` 비밀번호)이 근거(추정). 확인 명령 = `db2 get dbm cfg | grep -i authentication`.

## ④ Db2 LUW 고유

**결론**: 접속은 포트 50000 · RDBNAM 18바이트 공백 채움(Db2 DB 이름은 8자 이하) · PKGNAMCSN = `NULLID` / `SYSSH200` / 일관성 토큰 `SYSLVL01`(ASCII 16진 `5359534C564C3031`) / 섹션 번호로 동적 SQL을 처리한다(pydrda의 기본값 그대로 쓰면 된다).
DRDA에는 자동 커밋이 없다 → 문장마다 **RDBCMM을 묶어 보낸다**. 취소는 **새 연결 + INTRDBRQS(ACCRDBRM의 RDBINTTKN)** 가 표준 경로이고, 대안은 `ADMIN_CMD('FORCE APPLICATION')` / `WLM_CANCEL_ACTIVITY`다.

### 접속·식별 값
| 항목 | Db2 LUW / 참조 구현 | 출처 |
|---|---|---|
| 포트 | 50000 (`db2c_db2inst1`) · TLS는 관례상 50001 | pydrda `connection.py` |
| RDBNAM | 18바이트 고정, 공백 채움. pydrda는 `(db+' '*18)[:18]`. Derby는 18자가 넘으면 확장형(길이 접두 SCLDTALEN)으로 보내며 최대 255(Derby 10.11 이상 서버는 1024). Db2 LUW DB 이름은 8자 이하라 늘 18 고정형이다 | Derby `NetPackageRequest.java` · `NetConfiguration`(`PKG_IDENTIFIER_FIXED_LEN=18`, `MAX_LEN=255`) |
| EXCSAT 관리자 수준 | pydrda: AGENT 10 · SQLAM 11 · CMNTCPIP 5 · RDB 12 · SECMGR 9 · UNICODEMGR 1208, 접속 뒤 CCSIDMGR 1208 | pydrda `connection.py` |
| ACCRDB | PRDID `SQL12010`(pydrda는 Db2 클라이언트를 흉내 냄 · Derby는 `DNCmmrrm`) · TYPDEFNAM `QTDSQLX86`(리틀엔디언) · TYPDEFOVR(CCSID 1208) · CRRTKN | pydrda `ddm.py` |
| CRRTKN | Derby: IPv4를 문자 8개로 + `.` + 포트 4자 + 시각 6바이트 = 19바이트(첫 글자는 SNA 대응표로 바꿈) | Derby `NetConnection.constructCrrtkn` |
| EXTNAM / PRDDTA | Derby: `"derbydnc"+스레드 이름` · PRDDTA = PRDID + 플랫폼 `"JVM"` 18자 + 응용 ID 20자 + 사용자 8자 등 (최대 255) | Derby `NetConfiguration` |
| 클라이언트 정보 | `CALL SYSPROC.WLM_SET_CLIENT_INFO(userid, wrkstn, appname, acctstr, workload)` 또는 `SET CLIENT ...` 레지스터. 접속 시 EXCSQLSET으로 보내는 방식은 확인 못 함(추정) | — |

### 패키지 · 섹션 (PKGNAMCSN)
| 항목 | 값 |
|---|---|
| 형식 | RDBNAM(18) ‖ RDBCOLID(18) ‖ PKGID(18) ‖ PKGCNSTKN(8) ‖ PKGSN(2바이트 빅엔디언). 하나라도 18자를 넘으면 셋 모두 확장형 |
| 컬렉션 | `NULLID` (CLI·JCC 동적 패키지의 기본 컬렉션 · Derby `COLLECTIONNAME`) |
| 패키지 | `SYSSH200` (pydrda 고정). 명명 규칙 = `SYS` + S(작은 패키지, 64 섹션)/L(큰 패키지, 384 섹션) + H(WITH HOLD)/N + 격리 수준 숫자(1=UR 2=CS 3=RS 4=RR, 추정) + 일련번호 2자. 격리 수준·커서 유지 조합마다 48개를 미리 바인드해 둔다(설치 기본) |
| 일관성 토큰 | `SYSLVL01`. 오류 메시지 `NULLID.SYSSH200.5359534C564C3031`에서 보듯 **ASCII 바이트**로 간다 |
| 섹션 번호 | pydrda는 `pkgsn=65` 하나만 쓴다(동시 커서 1개 가정). 커서를 여러 개 열려면 섹션을 할당자로 관리해야 한다(1..64 또는 1..384, 추정) |
| 없을 때 | SQL0805N(패키지 없음) → 서버에서 `db2 bind @db2cli.lst`로 바인드 |
- 출처: 패키지 명명·개수 https://www.ibm.com/support/pages/75-ways-demystify-db2-9-tech-tip-db2-cli-packages-demystified · SQL0805N 예시 https://forums.toadworld.com/t/connecting-benchmark-factory-v-6-5-1-to-db2-on-z-os-v9-1/7985

### 질의 · 형식
| 항목 | 내용 |
|---|---|
| QRYBLKSZ | pydrda 65535 · DRDA 허용 범위는 512 ~ 10,485,760(10 MB)이고 JCC 기본값은 32767로 알려져 있다(추정). 큰 결과는 **QRYBLKSZ를 키우고 MAXBLKEXT를 -1로** 두는 쪽이 왕복을 줄인다 |
| QRYROWSET | 스크롤 가능 커서와 행 단위 블록용. 앞으로만 읽는 커서는 불필요(pydrda는 보내지 않음) |
| QRYCLSIMP | pydrda는 보낸다(끝에 닿으면 서버가 암묵적으로 닫음 → CLSQRY 왕복 0) |
| TIMESTAMP | Db2 LUW `TIMESTAMP(p)`, p는 0..12, 기본 6. 전송 형식 = 문자열 `YYYY-MM-DD-HH.MM.SS.ffffff`(26바이트 · p=12면 32바이트, pydrda `_fdodta`). 높은 정밀도의 FD:OCA 길이 협상은 확인 필요(추정) |
| CALL 결과 집합 | EXCSQLSTT에 MAXRSLCNT/RSLSETFLG를 담아 보낸다 → 응답은 **RSLSETRM**(결과 집합마다 PKGSN 목록) + SQLCARD + SQLRSLRD, 그다음 결과 집합마다 OPNQRYRM + (SQLCINRD) + QRYDSC + QRYDTA가 이어진다 → 나머지는 CNTQRY로 읽는다(Derby 서버 `DRDAConnThread` 흐름 · Db2도 같다고 추정) |
| SET CURRENT SCHEMA | **EXCSQLIMM + SQLSTT `SET CURRENT SCHEMA = X`** 가 가장 단순하다. EXCSQLSET은 접속 직후 레지스터를 일괄 설정할 때 쓴다(pydrda `packEXCSQLSET`은 PKGNAMCSN과 SQLSTT 여러 개) |
| RDBCMTOK | 문장 실행 **중에** 서버 쪽 커밋(프로시저 안 COMMIT 등)을 허용할지 정하는 플래그다(pydrda는 EXCSQLIMM·EXCSQLSTT에 `0xF1`). **자동 커밋 기능이 아니다** |

### 자동 커밋 · 취소
| 항목 | 내용 |
|---|---|
| 자동 커밋 | DRDA에는 "작업 단위 자동 커밋" 플래그가 없다 → 클라이언트가 **EXCSQLIMM/EXCSQLSTT 뒤에 RDBCMM을 같은 DSS 체인으로** 보낸다(pydrda `commit()` = RDBCMM · `begin()` = 아무것도 안 함, "DRDA는 작업 단위를 암묵적으로 시작"). SELECT는 커서를 닫은 뒤(또는 QRYCLSIMP로 끝까지 읽은 뒤) 커밋한다. 수동 모드 = RDBCMM/RDBRLLBCK을 명시적으로 보낸다 |
| 취소 1 (프로토콜) | ACCRDBRM이 **RDBINTTKN**을 돌려주면 → **새 TCP 연결**에서 EXCSAT + **INTRDBRQS(RDBINTTKN)** 를 보낸다. 원래 연결은 살아 있고 문장은 SQL0952N(-952)으로 끝난다. JCC `interruptProcessingMode=1`이 "별도 연결로 서버에 중단을 알린다"는 방식이다. 토큰이 안 오면 이 길은 쓸 수 없다. Db2 LUW가 이 토큰을 주는지는 확인 필요(추정) |
| 취소 2 (SQL) | 다른 연결에서 `CALL SYSPROC.ADMIN_CMD('FORCE APPLICATION (h)')`(연결 자체를 끊음, 권한 SYSADM/SYSCTRL/SYSMAINT) 또는 `CALL WLM_CANCEL_ACTIVITY(h, uow_id, activity_id)`(문장만 취소, 연결 유지). `h`는 대상 연결에서 미리 `VALUES MON_GET_APPLICATION_HANDLE()`로 받아 Sess에 저장해 둔다 |
| 취소 3 (최후 수단) | 소켓 닫기(JCC `interruptProcessingMode=2`) → 연결 재수립이 필요하다 |
- 출처: INTRDBRQS·RDBINTTKN https://documentation.softwareag.com/adabas/cxx146/userguide/CONNXCDD32D/DB2_Distributed_Data_Management.htm · JCC 타임아웃·중단 https://www.ibm.com/support/pages/timeout-properties-ibm-data-server-driver-jdbc-and-sqlj · 취소 시 -952 https://www.ibm.com/docs/SSEPEK_12.0.0/admin/src/tpc/db2z_cancelclientdynamicsql.html · pydrda `connection.py`·`ddm.py` https://github.com/nakagami/pydrda

## 추정 항목 (실서버 캡처나 DRDA 원문으로 확인할 것)
1. SERVER 인증에서 SECMEC 9가 받아들여지는지 · 컨테이너 이미지의 실제 `AUTHENTICATION` 값 · SSL이 기본으로 꺼져 있는지.
2. AES 변형(ENCALG 0x1909 / ENCKEYLEN 0x190A 코드포인트 · DH 소수 크기 · 키와 IV 유도) — Open Group DRDA Vol.3 원문 필요.
3. EUSRIDPWD 평문의 인코딩(EBCDIC/UTF-8)과 SECTKN 두 개의 순서.
4. USRSSBPWD(8)를 Db2 LUW가 지원하지 않는다는 점 · SECMEC 16/18 번호.
5. 패키지 이름의 격리 수준 숫자 대응(1~4) · 섹션 상한 64/384 · 다중 커서용 섹션 할당 방식.
6. QRYBLKSZ 범위(512 ~ 10 MB)와 JCC 기본값 32767 · TIMESTAMP(12) 전송 길이.
7. CALL 결과 집합 응답 순서가 Derby와 같은지 · Db2 LUW가 RDBINTTKN을 주는지(INTRDBRQS 지원 여부).
8. 접속 시 클라이언트 정보 전달 방식(EXCSQLSET 또는 `WLM_SET_CLIENT_INFO`).

---

> 조사 = 웹 + 얕은 clone(스크래치패드 `src/` · 읽기 전용). 줄 수 = `wc -l`(테스트 제외 · 주석 포함). **참고 = 읽기만 · 복사 금지**(라이선스와 무관하게 설계 원칙).

## ③ 순수 구현체 참고

**결론** — 지금 읽을 가치가 가장 큰 것은 **go-db2(MIT · Go 1만 줄 · Db2 LUW 전용 · 2026-10 활발)** 와 **pydrda(MIT · Python 2.3천 줄 · Db2 전용으로 정리됨)** 다. 프로토콜 흐름과 상태 기계의 "교과서"는 **Derby 클라이언트(Apache-2.0 · 2.4만 줄)** 지만 Derby는 2025-10 은퇴했고 Db2 LUW와 세부(타입 정의·패키지·LOB)가 다르다. 순수 Rust 선례 **justybase-db2**(MIT · alpha · 2.6만 줄 · go-db2 파생)가 이미 있어 "같은 문제를 Rust로 어떻게 쪼갰나"의 대조군으로 쓸 만하다.

### 구현체 표

| 구현 | URL | 언어 | 라이선스 | 유지 | Db2 LUW 지원 수준 |
|---|---|---|---|---|---|
| **pydrda** | <https://github.com/nakagami/pydrda> | Python 3.11+ | MIT | ✅ 마지막 커밋 2026-10-11 · 490 커밋 | 접속 SECMEC 3(USRIDPWD)·9(EUSRIDPWD = DH 256비트 + DES/CBC) · TLS(`use_ssl`) · SELECT/DML/DDL · `?` 바인드(SQLDTA/FDODSC) · LOB(EXTDTA · 응답 체인 함께 보냄) · DECFLOAT(DPD 인코딩) · commit/rollback · asyncio 판. **Derby 지원은 2026-08-07 커밋 `c961917`로 제거** |
| **Apache Derby 네트워크 클라이언트** | <https://github.com/apache/derby/tree/trunk/java/org.apache.derby.client/org/apache/derby/client/net> | Java | Apache-2.0 | ❌ **2025-10-10 은퇴 의결(읽기 전용)** · <https://db.apache.org/derby/> | 대상은 Derby 서버(DRDA 대화 가능 · Db2용 JCC와 상호운용 목적으로 설계). 인증 3·4·8·9 · 바인드 · LOB(EXTDTA·로케이터) · XA · 스크롤 커서 · 다중 행 페치 전부 |
| **go-db2** | <https://github.com/go-db2/go-db2> | Go(`CGO_ENABLED=0`) | MIT | ✅ 마지막 커밋 2026-10-14 · 140 커밋 · PR 19 | SECMEC 3·9 · Kerberos(7/11) · TLS 1.2+·mTLS · `?` 바인드 · 배치 DML · BLOB/CLOB/DBCLOB(EXTDTA) · XML · 트랜잭션 · **취소(SQLINTR)** · CALL(OUT/INOUT·다중 결과셋) · trusted context · mock 서버 시험 + Db2 Community 컨테이너 실시험 |
| **justybase-db2** (crates.io) | <https://github.com/justybase/JustyBase.Db2.Rust> · <https://crates.io/crates/justybase-db2> | **Rust** (tokio 비동기) | MIT | 🚧 0.1.0-alpha.3 (2026-10-24) · 다운로드 27 | SECMEC 3·7?·9 · TLS 1.2/1.3(tokio-rustls·ring) · 바인드 · LOB/XML 스트리밍·로케이터 · CALL 결과셋 · 취소(SQL interrupt) · 풀 · 실시험 기준 Db2 LUW 12.1.5 Linux. 의존 = tokio·tokio-rustls·rustls-native-certs·`des`·`ring`·`crypto-bigint`·tempfile·zeroize. **go-db2를 주 참고로 파생**(THIRD-PARTY-NOTICES) |
| obaydullahmhs/go-db2 | <https://github.com/obaydullahmhs/go-db2> | Go | Apache-2.0 | ❌ 커밋 3 · "WIP" | 기초만(접속·준비문 표방) — 참고 가치 낮음 |
| `db2` (crates.io) | <https://github.com/IBM/db2> | Rust | Apache-2.0 | 자리 맡기 0.0.2(크레이트 1.2 KB) | 내용 없음 · "IBM 공식" 이름 선점만 — 향후 IBM 공식 Rust 드라이버 신호일 수 있음(추정) |
| `ibm_db` (crates.io) | <https://github.com/ibmdb/rust-ibm_db> | Rust | Apache-2.0 | ✅ 1.0.8 | **순수 아님** — IBM CLI 드라이버(clidriver) 래퍼 · ODBC 계열 |
| `sqlx-db2` | crates.io | — | — | 0.0.0 자리 맡기(2020) | 내용 없음 |
| Go 기타 | `github.com/ibmdb/go_ibm_db` · `github.com/asifjalil/cli` | Go | — | — | **둘 다 cgo + clidriver** (Go wiki "uses cgo" 확인) |
| Node.js | `ibm_db`(npm) · `idb-connector` | JS | — | — | **순수 DRDA 구현 못 찾음** — `ibm_db`는 설치 시 clidriver 내려받음 · idb-connector는 IBM i 전용 네이티브 |
| Java JTOpen | <https://github.com/IBM/JTOpen> | Java | IPL-1.0 | ✅ | **DRDA 아님** — IBM i "database host server" 프로토콜(포트 8471) · Db2 LUW 불가 |
| Mapepire | crates.io `mapepire` | Rust | — | ✅ | IBM i 전용 · WebSocket 게이트웨이 · 무관 |
| .NET | `Net.IBM.Data.Db2` | C# | IBM 독점 | ✅ | **네이티브 clidriver 사용** — 순수 아님 |
| Ruby | `ibm_db` gem | Ruby | — | — | clidriver 래퍼 · 순수 구현 못 찾음 |
| **IBM JCC (db2jcc4.jar · Type 4)** | IBM Data Server Driver for JDBC | Java | **IBM 독점**(재배포는 IBM 라이선스 조건 · 역공학 금지 조항 추정) | ✅ | 순수 Java DRDA · 사실상 기준 구현 — **코드 열람·역컴파일 금지 대상**, 동작 비교(값 대조)용으로만 |

### 주요 파일과 내용

| 구현 | 파일(줄) | 내용 |
|---|---|---|
| pydrda | `drda/__init__.py`(221) | PEP-249 예외 계층 · `connect(host, database, port, user, password, use_ssl, ssl_client_cert_path, timeout)` → `Connection` |
| | `drda/connection.py`(544) | `Connection.__init__` = EXCSAT → ACCSEC(SECMEC 9면 DH 공개키 동봉) → ACCSECRD 해석(`_parse_accsecrd` · 서버가 다른 secmec 내면 재협상) → SECCHK → ACCRDB · `_parse_response`(응답 체인 순회 · SQLCARD/SQLDARD/OPNQRYRM/QRYDSC/QRYDTA/EXTDTA) · `_execute`(EXCSQLIMM/EXCSQLSTT) · `_query`(PRPSQLSTT+OPNQRY+CNTQRY 묶어 보냄 · "Db2는 OPNQRYRM 뒤 항상 CNTQRY 필요") · commit/rollback(RDBCMM/RDBRLLBCK) · `_set_variables`(EXCSQLSET) |
| | `drda/codepoint.py`(225) | **코드포인트 상수 평면 목록**(EXCSAT 0x1041 · ACCSEC 0x106D · SECCHK 0x106E · QRYDSC 0x241A · SQLDARD 0x2411 · FDODSC 0x0010 …) — 가장 압축된 표 |
| | `drda/ddm.py`(593) | DSS 읽기/쓰기(`read_dss`·`write_request_dss` · 체인·같은 상관 ID 플래그) · `pack*` 명령 빌더 전부 · `parse_sqlcard` · `parse_sqldard`/`_parse_column_db2`(열 설명) · `_fdodsc`/`_fdodta`/`packSQLDTA`(바인드 FD:OCA 기술자 + 데이터) · `packEXTDTA` · PKGNAMCSN(NULLID 패키지 · 섹션 번호) |
| | `drda/utils.py`(427) | `read_field`(QRYDTA 행 디코딩 · 타입별 · 엔디언) · packed decimal · DECFLOAT DPD 인코드/디코드 · `escape_parameter` |
| | `drda/consts.py`(110) · `secmec9.py`(61) | SQL 타입 번호·SECMEC 상수 · DH 소수/생성원(256비트) + DES 키 유도(세션키 12..20바이트 · IV = 서버 토큰 12..20) |
| | `misc/drdaproxy.py` | **패킷 덤프 프록시** — 실서버와 오가는 DSS를 해석해 출력 (캡처·재생 시험 자료 만들기에 바로 쓸 발상) |
| Derby | `net/NetConnection.java`(2014) | 접속 상태 기계 · secmec 협상(flowUSRIDPWD/EUSRIDPWD/USRSSBPWD 분기) · 재시도 |
| | `net/NetConnectionRequest.java`(637) / `NetConnectionReply.java`(3331) | EXCSAT·ACCSEC·SECCHK·ACCRDB 빌더 / **응답·오류 코드포인트 전수 해석**(RDBNFNRM·SECCHKRM·MGRLVLRM·SYNTAXRM 등 — 오류 응답 처리의 원장) |
| | `net/NetStatementRequest.java`(1891) / `NetStatementReply.java`(2386) | PRPSQLSTT·DSCSQLSTT·EXCSQLSTT·OPNQRY·SQLDTA·EXTDTA(LOB 바인드) / SQLDARD·OPNQRYRM·QRYDSC 해석 · CALL 결과셋(RSLSETRM) |
| | `net/NetCursor.java`(1219) · `NetResultSetRequest/Reply` | QRYDTA 행 블록 파싱 · CNTQRY · 남은 행·종료(ENDQRYRM) · 스크롤 |
| | `net/CodePoint.java`(653) · `CodePointNameTable.java` | 코드포인트 + 이름 표(로그용) |
| | `net/Typdef.java`(1213) · `FdocaConstants.java` | **TYPDEFNAM(QTDSQLX86/QTDSQLJVM/QTDSQL370)별 엔디언·CCSID** · FD:OCA 타입 → JDBC 타입 매핑 |
| | `net/Request.java`(1506) · `Reply.java`(1359) · `DssConstants.java` | DSS 프레이밍·분할(32767 넘는 객체 연장 길이)·체인 플래그 |
| | `am/EncryptionManager.java` · `am/Decimal.java` · `am/DateTime.java` · `am/Sqlca.java` | ★ `EncryptionManager`는 `net`이 아니라 **`client/am`** 에 있다 — DH·DES·SHA-1 강한 패스워드 대체(SECMEC 8) / packed decimal / 날짜·시간 문자열 / SQLCA |
| go-db2 | `network/`(3.3천) · `network/security/`(527 · `secmec9.go`·`kerberos.go`·`tls.go`) · `converters/`(2.0천) · 루트(2.1천) | codepoint·ddm·dss·session 분리 · 보안은 하위 폴더 · 형 변환 별도 — **Rust 모듈 경계 설계에 가장 가까운 꼴** |

### 무엇을 위해 무엇을 읽나

| 목적 | 1순위 | 보조 |
|---|---|---|
| 전체 흐름·명령 순서(접속 4단계 · 준비→열기→계속 페치) | **Derby** `NetConnection*` · `NetStatement*` | pydrda `connection.py`(짧아 한눈에) |
| 압축된 코드포인트 표 | **pydrda `codepoint.py`** | Derby `CodePoint.java`(+이름 표) |
| Db2 LUW 고유 동작(CNTQRY 필수 · LOB 체인 · SQLDARD 2단 · NULLID 패키지) | **pydrda 주석** · **go-db2** | justybase-db2 `docs/` |
| FD:OCA/QRYDSC·SQLDTA 바이트 배치 | Derby `Typdef`·`NetCursor` | pydrda `ddm._fdodsc`·`utils.read_field` |
| SECMEC 9 (DH+DES) | pydrda `secmec9.py`(61줄) | Derby `am/EncryptionManager` · go-db2 `security/secmec9.go` |
| 오류 응답·SQLCARD 전수 | Derby `NetConnectionReply` | pydrda `parse_sqlcard` |
| 취소·CALL·XML·배치 | **go-db2** | justybase-db2 |
| Rust로의 모듈 분할·비동기 경계 대조 | justybase-db2(`src/network`·`security`·`converters`·`replay.rs`) | — |
| 명세 원전 | The Open Group **DRDA V5 Vol.1(DRDA)·Vol.2(FD:OCA)·Vol.3(DDM)** (C114/C112/C113 · 무료 열람) | IBM Docs "DRDA reference" |

## ⑤ 범위 제안

**결론** — 1단계는 pydrda가 덮는 범위(= 접속·SECMEC 3/9·TLS·SELECT/DML/DDL·`?` 바인드·기본 타입·NULL·commit/rollback·SQLCARD)를 Rust로 **약 4~6천 줄**에 담고, 2단계에서 go-db2 수준(LOB·CALL 결과셋·취소·XML·DECFLOAT·페치 조율)으로 **+4~6천 줄** 올린다. CI는 **이미 쓰는 Db2 Community 컨테이너 + 캡처 재생(replay) 오프라인 시험**이 주력이고, Derby 네트워크 서버는 은퇴·방언 차이·pydrda도 지원을 끊은 점에서 **CI 대상으로 권하지 않는다**.

### 단계별 범위

| 단계 | 항목 | 근거(누가 구현) | 비고 |
|---|---|---|---|
| **1** | DSS 프레이밍·체인·연장 길이 · 코드포인트 표 | 전부 | 순수 함수 + 바이트 고정 시험 |
| 1 | 접속 EXCSAT(MGRLVLLS)/ACCSEC/SECCHK/ACCRDB · secmec 재협상 | pydrda·Derby | ACCRDB의 TYPDEFNAM = QTDSQLX86(리틀 엔디언) · CCSID 1208(UTF-8) 요청 |
| 1 | **USRIDPWD(3)** + **EUSRIDPWD(9)** | pydrda `secmec9` | 9 = DH 256비트 modpow + DES-CBC — DES(~250줄)·고정폭 bigint(~150줄) **자체 구현** 또는 `des` 크레이트(DR-3 드라이버 예외 원장 등재) · 서버 `AUTHENTICATION=SERVER_ENCRYPT`면 9 필수(추정 · AES 변형은 2단계) |
| 1 | **TLS** | pydrda `use_ssl` · go-db2 | DR-3 예외로 **rustls 0.23 + aws-lc-rs 재사용**(D-348 `nsql-net`·mysql·oracle-thin이 이미 끌어옴 → 추가 TLS 스택 0) · 루트 = webpki-roots + 사용자 CA 파일 · Db2 TLS = 별도 포트(SSL_SVCENAME)에 처음부터 TLS(STARTTLS 아님) |
| 1 | SELECT(PRPSQLSTT→OPNQRY→CNTQRY…ENDQRYRM) · DML/DDL(EXCSQLIMM/EXCSQLSTT) · 행 수(SQLCARD `sqlerrd[2]`) | pydrda | NULLID 동적 패키지(SYSSH200 등) · 섹션 번호 관리 |
| 1 | `?` 바인드(SQLDTA + FDODSC) | pydrda `_fdodsc/_fdodta` | 1단계는 비-LOB 타입만 |
| 1 | 타입: SMALLINT/INTEGER/BIGINT · REAL/DOUBLE · **DECIMAL(packed)** · CHAR/VARCHAR(+FOR BIT DATA) · GRAPHIC 계열 · DATE/TIME/TIMESTAMP(문자열 형식) · BOOLEAN · NULL 표시자 | pydrda `utils` · Derby `am/Decimal`·`DateTime` | DR-33 `ResultSet` 세그먼트에 바로 쓰기 · 소수 = 문자열로 보존 |
| 1 | commit/rollback(RDBCMM/RDBRLLBCK) · 자동 커밋 끄기 | pydrda | DR-34 `gate_open` 앞단은 기존 그대로 |
| 1 | 오류: SQLCARD(SQLCODE·SQLSTATE·토큰) · 응답 메시지(SYNTAXRM·PRCCNVRM·RDBNACRM·SECCHKRM 등) → `DbError` | Derby `NetConnectionReply` | 원문 + 코드 보존 |
| **2** | LOB 읽기·쓰기(EXTDTA · 큰 객체 분할) · 로케이터 | pydrda(읽기·바인드) · Derby · go-db2 | 지연 로딩 원칙(61 §1-8) — 그리드는 앞부분만 |
| 2 | CALL + OUT/INOUT + 동적 결과셋(RSLSETRM·OPNQRY 다중) | Derby · go-db2 | |
| 2 | **취소** — DRDA 인터럽트(INTRDBRQS/SQLINTR · 별도 연결로 보냄) 우선 · 폴백 = 별도 세션 `CALL SYSPROC.ADMIN_CMD('FORCE APPLICATION (...)')`(권한 필요) | go-db2 · justybase | ■ 버튼 = 막힌 상태를 푸는 예외 동작 |
| 2 | XML · DECFLOAT(DPD) · 다중 행 페치 조율(QRYBLKSZ · MAXBLKEXT · QRYROWSET) | pydrda(DECFLOAT) · go-db2 | 26 §8 네트워크 상한 표 갱신 |
| 2 | SECMEC AES 변형 · Kerberos · trusted context | go-db2 | 요청 있을 때만 |

### 노력 추정 (유추)

| 단계 | 기준 | Rust 예상 줄 수 | 비고 |
|---|---|---|---|
| 1 | pydrda 전체 2.3천 줄(Python) × Rust 밀도 1.8~2.5 + 시험 | **4~6천 줄**(시험 포함 · 시험 ~40%) | DES·bigint 자체 구현 시 +0.5천 |
| 2 | go-db2 1.04만 줄 − 1단계 상당분 | **+4~6천 줄** | justybase-db2(2.6만 줄)는 풀·임포트·관리 API까지 포함한 상한 |
| 합계 | — | **8~12천 줄** · 1단계 3~5주 · 2단계 4~6주(1인 · 추정) | 기존 드라이버 포트(`Box<dyn Session>`)·cdylib 확장 껍데기는 재사용 |

### 시험 전략

| 층 | 방법 | 비고 |
|---|---|---|
| 단위 | 코드포인트·DSS·FD:OCA·packed decimal·DECFLOAT·DES/DH = 순수 함수 + 알려진 바이트 벡터 | 단순 모델 난수 대조(61 규칙) |
| **오프라인 재생** | 실서버 왕복을 **캡처해 저장한 DSS 바이트로 재생**(pydrda `misc/drdaproxy.py` 발상 · justybase `replay.rs` · go-db2 `mockserver_test.go`) | CI에서 컨테이너 없이 3-OS 전부 · 회귀의 주력 |
| 실서버 | **Db2 Community 컨테이너**(`icr.io/db2_community/db2` · privileged · 기동 수 분 · RAM 4 GB급) — `integration.yml`이 이미 Db2 잡 운영(ODBC 경로) | 같은 잡에 네이티브 드라이버 시험 추가 · TLS 포트도 켜서 1회 |
| 값 대조 | 같은 쿼리를 기존 ODBC(clidriver) 경로와 네이티브 경로로 돌려 셀 단위 비교 | justybase가 JCC·.NET과 하는 방식의 축소판 · JCC 코드는 열지 않음 |
| Derby 네트워크 서버 | **권하지 않음** — 2025-10 은퇴(보안 수정 없음) · TYPDEF(QTDSQLJVM)·패키지·LOB 체인·SQLDARD 세부가 Db2 LUW와 다르고 pydrda도 2026-08에 Derby 분기를 지웠다(CLOB 페치 정지 등 별도 분기 필요했음) | 굳이 쓴다면 JVM만 있으면 뜨는 "프레이밍·접속 연기 시험" 정도 |

### 추정 항목

- 줄 수·기간 = pydrda·go-db2·justybase 크기에서의 유추(실측 아님).
- SECMEC 9의 Db2 LUW 쪽 세부(DES 외 AES 변형의 DH 키 길이 · `SERVER_ENCRYPT`일 때 3 거부 여부)는 문서·실서버로 확인 전.
- IBM JCC 라이선스의 역공학 금지 조항 존재 = 통상 IBM 라이선스 관행으로 추정(원문 미확인).
- crates.io `db2`(IBM/db2 · 0.0.2)가 향후 IBM 공식 순수 드라이버가 될지 = 추정.
- justybase-db2 SECMEC 7 표기는 README 요약 기준(일반적으로 7 = 암호화 사용자 ID/패스워드 계열이 아닌 다른 의미일 수 있어 미검증).
- 취소의 DRDA 인터럽트가 Db2 LUW 12.1에서 별도 소켓을 요구하는지 = go-db2·justybase 주장 기준 · 미검증.

## ⑥·⑦ 머리말

> 원천 경로 = 참고 구현체 로컬 사본(§③ 구현체 표의 저장소 · 참고만 · 코드 복사 없음) 기준 상대 경로 · 줄 번호 = 10-25 사본. `derby/…/client/` = `derby/java/org.apache.derby.client/org/apache/derby/client/` · `jb` = justybase-db2 · `godb2` = go-db2.
> 원천 4개 중 **CALL 결과 집합을 실제로 구현한 것은 derby와 jb뿐** — go-db2는 OUT 매개변수만(결과 집합 = 빈 Rows) · pydrda `callproc` = `NotSupportedError`.
> ★ 10-25 실측(db2.yml 40149595519 · 0c062e15) = CI 서버(Db2 12.1 Community · 기본 `AUTHENTICATION=SERVER`)는 ACCSEC(9) 요청에 ACCSECRD `SECMEC` 목록 **{3, 5}** + `SECCHKCD 0x01`(요청 방식 미지원) · SECTKN 없음으로 답한다 = **9 미지원** → 아래 7-3 재협상(목록에서 3 선택)이 실제로 필요한 경로. 9의 실서버 검증은 서버를 `SERVER_ENCRYPT`로 바꾼 별도 단계가 있어야 가능(추정).

## ⑥ CALL 결과 집합 (T-426 2차 ③ · 10-25 조사)

**결론(3줄)**
1. 요청 = `EXCSQLSTT`(준비된 CALL 섹션) + `OUTEXP 0xF1`(OUT/INOUT이 있을 때) + `QRYBLKSZ` + `MAXRSLCNT 0xFFFF` + `RSLSETFLG 0x04`(확장 SQLDA) [+ `MAXBLKEXT`/`QRYROWSET`] → 이어서 `SQLDTA`(입력 값).
2. 응답 한 체인 = `RSLSETRM{SVRCOD, PKGSNLST[PKGNAMCSN…]}` → `SQLCARD` **또는** `SQLDTARD{FDODSC, FDODTA}`(OUT 값 · 둘은 서로 배타) → `SQLRSLRD`(결과 집합 수·요약) → 결과 집합마다 `OPNQRYRM{QRYINSID}` [+`SQLCARD`] [+`SQLCINRD`] + `QRYDSC` + `QRYDTA`* [+`ENDQRYRM`+`SQLCARD`] → (OUT LOB이면) `EXTDTA`.
3. 이어 받기 = 결과 집합 i마다 `PKGSNLST[i]`의 PKGNAMCSN(64B · 섹션 번호 포함) + 그 `OPNQRYRM`의 `QRYINSID`로 `CNTQRY`, 닫기 = 같은 두 값으로 `CLSQRY`. OUT 값의 형식은 `SQLDTARD` 안의 `FDODSC`가 스스로 설명한다(DARD 불필요).

### 6-1. EXCSQLSTT 요청 매개변수 비교

| 매개변수 (코드) | derby `writeExecuteCall` | jb `pack_execute_sql_statement_for_result_sets` | go-db2 `PackEXCSQLSTT` | pydrda |
|---|---|---|---|---|
| PKGNAMCSN (0x2113) | 준비된 섹션 | `SYSSH200`/`SYSLVL01`/섹션 | 같음 | (CALL 미지원) |
| RDBCMTOK (0x2105) | — | `0xF1` | `0xF1` | — |
| OUTEXP (0x2111) | 매개변수 1개 이상이면 `TRUE=0xF1`, 아니면 생략 | OUT/INOUT 있으면 `0xF1` 아니면 `0xF0` | **보내지 않음** | — |
| QRYBLKSZ (0x2114) | `MAX_DSS_LEN`(32767) | 세션 블록 크기(4B) | — | — |
| MAXBLKEXT (0x2141) | `-1`(0xFFFF = 무제한) | 행 집합 크기 줄 때만 `0xFFFF` | — | — |
| MAXRSLCNT (0x2140) | `0xFFFF`(`MAXRSLCNT_NOLIMIT`) · 배치면 생략 | 호출부가 `u16::MAX` | — | — |
| RSLSETFLG (0x2142) | `0x04`(`RSLSETFLG_EXTENDED_SQLDA`) | `0x04` | — | — |
| QRYROWSET (0x2156) | fetchSize(항상) | 선택 | — | — |
| PRCNAM (0x2138) | 프로시저 이름이 있으면 | — | — | — |

인용: `derby/…/client/net/NetStatementRequest.java:271-304`(writeExecuteCall · `outputExpected = numParameters > 0` 287행) · `:1596-1600`(buildOUTEXP) · `:1667-1669`(QRYBLKSZ) · `:1686-1691`(MAXRSLCNT) · `:1729-1731`(RSLSETFLG) · `:1838-1840`(플래그 0x04) · `net/CodePoint.java:38-48,56`(0xFFFF · 0x04 · TRUE=0xF1) · `jb/src/network/ddm.rs:554-579` · 호출 `jb/src/network/session.rs:5415-5428` · `godb2/network/ddm.go:548-550,310`.

### 6-2. 응답 순서(derby가 규정대로 순서 고정 파싱)

| # | 객체 | 내용 | 근거 |
|---|---|---|---|
| 0 | (선택) RDBUPDRM · ENDUOWRM | 트랜잭션 성분 | `NetStatementReply.java:287-296` |
| 1 | **RSLSETRM 0x2219** | `SVRCOD` + **PKGSNLST 0x2139** = PKGNAMCSN 반복(결과 집합 순) | `:300-301` → `parseRSLSETRM :1160-1200` · `parsePKGSNLST :1766-1776` |
| 2 | (선택) TYPDEFNAM/TYPDEFOVR | | `:381` |
| 3 | **SQLCARD** 또는 **SQLDTARD 0x2413** | 매개변수가 있으면 SQLDTARD(안에 SQLCA 포함) — 상호 배타 | `:383-396` |
| 4 | **SQLRSLRD 0x240E** | `SQLNUMGRP`(2B) + 집합마다 `SQLRSLOCATOR`(4B) · `SQLRSNAME`(VCM+VCS) · `SQLRSNUMROWS`(4B) | `:402` → `parseSQLRSLRDarray :1885-1892` · `parseSQLRSGRP :2279-2285` |
| 5 | 집합마다: **OPNQRYRM 0x2205**(`QRYINSID 0x215B` 8B) → (선택) SQLCARD(경고) → (선택) **SQLCINRD 0x240B**(열 정보 = 확장 SQLDA) → **QRYDSC** → QRYDTA* → (선택) ENDQRYRM | | `parseResultSetCursor :432-470` · `parseOPNQRYRM :782,874-878` |
| 6 | (선택) EXTDTA | OUT 매개변수 LOB | `:419-424` |

- 이 전부가 **EXCSQLSTT 한 요청에 대한 한 응답 체인**(같은 상관 번호의 연쇄 DSS)으로 온다. derby는 1~6을 한 번의 읽기 체인에서 연속 파싱한다(`parseResultSetProcedure :362-427`).
- jb는 순서에 의존하지 않고 코드포인트별로 분기하며(`session.rs:5455-5537`), `SQLRSLRD` 개수 = `PKGSNLST` 개수 = `OPNQRYRM` 개수를 검사한다(`:5568-5573`). jb의 모의 서버는 RSLSETRM → SQLRSLRD → SQLDTARD → OPNQRYRM 순으로 보낸다(`session.rs:13427-13460`) — 실서버 순서(3과 4)와 다르므로 **순서 비의존 파싱이 안전**.
- jb 주석: Db2 11.5.8은 OUT 매개변수의 EXTDTA를 OPNQRYRM/SQLCINRD/QRYDSC **뒤 · 첫 QRYDTA 앞**에 보낸다(`session.rs:5507-5511`) → 첫 QRYDTA 이전의 EXTDTA는 OUT 값으로 귀속.
- SQLRSLRD의 `SQLNUMGRP` 바이트 순서 = ACCRDB `TYPDEFNAM`을 따른다: jb(QTDSQLX86) = 리틀 엔디언(`ddm.rs:953-964` · 최소 2+12×n 바이트), derby(QTDSQLJVM) = 빅(`NetConnectionReply.java:2572-2574` readShort).
- PKGNAMCSN 고정 64B 배치: RDBNAM 18 · RDBCOLID 18 · PKGID 18 · PKGCNSTKN 8 · PKGSN 2(BE) (`jb/src/network/ddm.rs:966-988` 파싱 · `:581-600` 재포장 = 길이 68 헤더 포함). 결과 집합마다 **섹션 번호가 다르다**(모의 65/66 · `session.rs:13416-13424`).

### 6-3. 이어 받기 · 닫기

| 명령 | derby (`NetResultSetRequest.java`) | jb (`ddm.rs`) |
|---|---|---|
| CNTQRY | `buildCoreCNTQRY :189-209` = PKGNAMCSN(결과 집합 섹션) · QRYBLKSZ · [MAXBLKEXT -1] · QRYINSID · [QRYROWSET] · LOB 있으면 `RTNEXTDTA=RTNEXTALL` (`:166-186`) | `pack_continue_procedure_result_set :629-642` = PKGNAMCSN · QRYBLKSZ · QRYINSID · `RTNEXTDTA 0x02` · `FREPRVREF 0xF0` |
| CLSQRY | `buildCLSQRY :156-164` = PKGNAMCSN + QRYINSID | `pack_close_procedure_result_set :644-652` 같음 |

- jb 흐름: 집합 순서대로 `ended`(ENDQRYRM 또는 SQLCODE +100)까지 CNTQRY 반복 → 행 해독 · EXTDTA 붙이기 → CLSQRY → 다음 집합 (`session.rs:5575-5636`). 자동 커밋이면 끝에 커밋(`:5561`).
- CNTQRY의 QRYINSID는 **그 집합의 OPNQRYRM에서 받은 값**(`session.rs:5482-5487` · derby `:874-878`).

### 6-4. OUT 매개변수 해독

- `SQLDTARD` = `FDODSC`(0x0010 · QRYDSC와 같은 FD:OCA 기술자 · 0x76 = SQLDTAGRP 정의) + `FDODTA`(0x147A). derby `parseSQLDTARD :1211-1246` · `parseFDODSC :1255-1258`(QRYDSC와 같은 `parseSQLDTARDarray`) · `parseFDODTA :1499-1510`.
- `FDODTA` = 앞머리 SQLCARD 행(널 지시자 `0xFF`면 SQLCA 없음 · 아니면 SQLCA 전체) → 데이터 그룹 널 지시자 `0x00` → 필드마다(널 허용이면 지시자 1B +) 값. go-db2는 첫 바이트가 0xFF면 2B를 건너뜀(`godb2/converters/decoders.go:540-620` · 603-609행 근처) · jb는 0xFF로 시작하지 않으면 `[0xFF,0]`을 앞에 붙여 행 해독기를 재사용(`jb/src/network/session.rs:6963-6967`).
- 형식은 FDODSC가 담으므로 PRPSQLSTT/DSCSQLSTT의 SQLDARD는 **입력 SQLDTA 작성용**(매개변수 형식 497 등)으로만 필요. go-db2 `ExecWithParams`는 SQLDARD가 먼저 온 체인 뒤 두 번째 체인을 더 읽는다(`godb2/network/session.go:865-887`).
- jb 모의 예: FDODSC `06 76 D0 03 00 04`(INTEGER 널 허용) · FDODTA `FF 00 00 2A 00 00 00`(LE 42) (`session.rs:13432-13438`).

### 6-5. 추정 항목
- go-db2가 OUTEXP 없이도 SQLDTARD를 받는다는 것은 코드만으로는 확인 못 함(Db2가 OUT 매개변수가 있으면 무조건 보내는지) — **추정**: 규정대로 `OUTEXP 0xF1`을 보내는 편이 안전.
- SQLRSLRD 요약의 `SQLRSNAME`이 SQLAM 6/7에서 길이가 다른 점(derby 주석 2282행) — 정확한 차이는 DRDA Vol.1(C112) SQLRSGRP 정의로 확인 필요(**추정**).
- `RSLSETFLG` 비트 의미(0x04 외 표준/라이트 SQLDA) = derby 주석(`CodePoint.java:47`)뿐 · 원문 미확인.

## ⑦ EUSRIDPWD(SECMEC 9) (T-426 2차 ④ · 10-25 조사 · 구현 a7e599ac)

**결론(3줄)**
1. DH 256비트: p = `C62112D73EE613F0947AB31F0F6846A1BFF5B3A4CA0D60BC1E4C7A0D8C16B3E3`, g = `4690FA1F7B9E1D4442C86C9114603FDECF071EDCEC5F626E21E256AED9EA34E4` — pydrda · derby · go-db2 · jb **네 곳 모두 일치**.
2. 클라이언트 공개 키(32B BE)를 **ACCSEC의 SECTKN**에, 서버 공개 키는 **ACCSECRD의 SECTKN**으로 받는다 → 공유 비밀(32B BE)의 `[12..20)` = DES 키 · 서버 SECTKN `[12..20)` = IV · DES-CBC/PKCS5.
3. SECCHK = `SECMEC=9` · `RDBNAM` · `SECTKN`(암호화 사용자 ID) · `SECTKN`(암호화 비밀번호) 순 · 평문 = 협상된 CCSID(기본 EBCDIC cp500 · derby는 UNICODEMGR 1208 협상 시 UTF-8).

### 7-1. DH 매개변수 대조

| 원천 | p | g | 개인 키 |
|---|---|---|---|
| pydrda `drda/secmec9.py:32,34` | 위 hex | 위 hex | `random.randrange(2, prime)`(37-38 · 비암호 난수) |
| derby `client/am/EncryptionManager.java:70-79,87-96` | 바이트 배열 `C6 21 12 D7 … B3 E3` 동일 | `46 90 FA 1F … 34 E4` 동일 | JCE DH, **지수 길이 255비트**(103, 145) |
| go-db2 `network/security/secmec9.go:17,19` | 동일 문자열 | 동일 | `[2, p-1)` crypto/rand(23-31) |
| jb `src/security/mod.rs:40-51` | 리틀 limb로 동일 | 동일 | 32B 난수 · 최상위 비트 0 = 255비트 · 범위 `[2, p-2]`(144-157) |

### 7-2. 절차와 바이트 규칙

| 단계 | 내용 | 근거 |
|---|---|---|
| 공개 키 | `g^x mod p` → **32B 빅 엔디언 고정**(33B면 앞 0 제거 · 짧으면 앞 0 채움) | derby `obtainPublicKey :195-233` · pydrda `connection.py:302` · jb `:159-165` |
| ACCSEC | `SECMEC(0x11A2)=9` · `RDBNAM` · **`SECTKN(0x11DC)` = 클라이언트 공개 키** | pydrda `ddm.py:330-334` · derby `flowKeyExchange NetConnection.java:683-691` · jb `session.rs:4563-4579` |
| ACCSECRD | `SECMEC` 목록 · (선택) `SECCHKCD` · **`SECTKN` = 서버 공개 키 32B** | pydrda `connection.py:219-238` · derby `setAccessSecurityData NetConnection.java:887-925` |
| 공유 비밀 | `S^x mod p` → 32B BE 정규화 | derby `generatePrivateKey :299-367` · pydrda `:45-46` |
| DES 키 | 공유 비밀 **`[12..20)`**(가운데 8B) | derby `:353-362` · pydrda `:60` · jb `:211` |
| 홀수 패리티 | derby만 각 바이트 LSB를 홀수 패리티로 조정(`keyParityCheck :270-297`) — DES는 패리티 비트를 키 일정에서 버리므로(PC-1) **결과 동일 · 생략 가능**. pydrda·jb·go-db2는 조정 안 함 | |
| IV | 서버 SECTKN **`[12..20)`** | derby `calculateEncryptionToken :247-268`(secmec 9 → `initVector[i+12]`) · pydrda `:59` · jb `:211` |
| 암호 | DES/CBC/PKCS5 · 사용자 ID와 비밀번호를 **각각 독립적으로**(둘 다 같은 IV에서 시작) | derby `encryptData :387-430`(`DES/CBC/PKCS5Padding` 421) · jb `des_cbc_encrypt :179-198` |
| 평문 인코딩 | pydrda = `cp500`(`connection.py:258` → `ddm.py:299-300`) · jb = `encode_cp500`(`session.rs:4719-4733`) · derby = `getCurrentCcsidManager()`(`NetConnection.java:1363-1375`) = 기본 EBCDIC, ACCSECRD 처리 뒤 서버가 UTF-8 CCSID 지원하면 UTF-8로 전환(`NetConnectionReply.java:2039-2044`) | |
| SECCHK | `SECMEC` · `RDBNAM` · `SECTKN`(사용자 ID) · `SECTKN`(비밀번호) — **순서 = 사용자 → 비밀번호** · USRID/PASSWORD 평문 필드 없음 | pydrda `ddm.py:294-303` · derby `NetConnectionRequest.java:316-334` · jb `ddm.rs:364-379` |

- 검증 벡터(jb 단위 시험 · Derby·OpenSSL 대조라고 적힘): 개인 키 c=`7f3a91c4…b0c9`/s=`1b2c3d4e…f90a` → 공개 키 · 공유 비밀 `303a7d3c…09af53` · `"SecretPass123"`(ASCII) → `a64a5e61af1a35a65d2bc8ec7ae550f4` · `"DB2INST1"` → `e3241d6a67a9646ce5fafe9f9bae7acc` (`jb/src/security/mod.rs:256-300`). 우리 구현의 왕복 시험에 그대로 쓸 수 있다(값 대조만 · 코드 복사 아님).

### 7-3. 서버가 9를 요구할 때(재협상)

| 원천 | 동작 |
|---|---|
| jb | 처음 기정(3)으로 ACCSEC → ACCSECRD의 SECMEC 목록이 `[요청값]`이 아니면 거절로 보고, 설정이 자동(0)이면 목록에 있는 **9, 없으면 7**로 **새 키 쌍을 만들어 ACCSEC 재전송**(같은 연결 · 자격 증명은 아직 안 보냄) · 두 번째도 거절이면 오류(`session.rs:4593-4627, 4658-4685`). |
| pydrda | 처음부터 9로 시작(`connection.py:257`) · ACCSECRD의 SECMEC가 다르면 그 값으로 ACCSEC를 다시 보내지만 **두 번째 ACCSECRD를 읽지 않고** 첫 SECTKN으로 SECCHK를 체인(`:311-331`) — 결함 가능(**추정**). |
| derby | 목록이 `[요청값]`이 아니면 예외 누적(자동 재시도 없음 · `NetConnection.java:901-935`). |
| go-db2 | 재시도는 하지만 ACCSEC에 SECTKN을 **넣지 않고** 공개 키를 SECCHK에 넣으며 비밀번호만 UTF-8로 암호화(`session.go:341-405` · `secmec9.go:81-110`) — DRDA 규정과 어긋남 · **참고 금지**. |

권장 흐름: ACCSEC(9, 클라이언트 공개 키) 단독 → ACCSECRD 확인(SECMEC=[9] · SECTKN 32B · SECCHKCD 0) → SECCHK + ACCRDB 체인. 거절 목록에 9가 있으면 새 키로 ACCSEC 재전송.

### 7-4. DES 구현 공개 원천(표는 복사하지 않음)

| 무엇 | 원천 |
|---|---|
| DES 알고리즘 전문 | **FIPS 46-3** <https://csrc.nist.gov/files/pubs/fips/46-3/final/docs/fips46-3.pdf> — 본문 "Enciphering"(IP · IP⁻¹ 표) · "The Cipher Function f"(E 비트 선택표 · 선택 함수 S1~S8 · 치환 P) · "Key Schedule"(PC-1 · PC-2 · 왼쪽 회전 횟수표 1,1,2,2,2,2,2,2,1,2,2,2,2,2,2,1) · 부록 1 "Primitive Functions"(S-상자 · P) · 부록 2 "Key Schedule Calculation"(PC-1 · PC-2) (절 이름은 **추정** — 원문 목차로 확인) |
| CBC 모드 | **FIPS 81** "DES Modes of Operation" <https://csrc.nist.gov/pubs/fips/81/final> · 일반 모드 = NIST SP 800-38A <https://csrc.nist.gov/pubs/sp/800/38/a/final> |
| 알려진 답 시험 | NBS SP 500-20 "Validating the Correctness of Hardware Implementations of the NBS DES" <https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nbsspecialpublication500-20e1980.pdf> · NIST SP 800-17(MOVS · DES 변수 평문/키/치환 시험) <https://csrc.nist.gov/pubs/sp/800/17/final> |
| 손으로 확인할 벡터(**추정** — 기억 기반, 위 원천으로 재확인) | ECB: 키 `133457799BBCDFF1` · 평문 `0123456789ABCDEF` → `85E813540F0AB405` · FIPS 81 부록 CBC 예: 키 `0123456789ABCDEF` · IV `1234567890ABCDEF` · 평문 `"Now is the time for all "` → `E5C7CDDE872BF27C 43E934008C389C0F 683788499A7C05F6` |
| SECMEC 9 설명 | Derby 위키 SecurityMechanism <https://cwiki.apache.org/confluence/display/DERBY/SecurityMechanism> · DRDA Vol.3 DDM(ACCSEC/SECCHK/SECTKN) = Open Group <https://publications.opengroup.org/> (요청자 표기 C112/C113/C114 · 권 번호 대응은 **추정**) |

### 7-5. 추정 항목
- Db2 LUW가 SECCHK 평문을 **항상** cp500으로 기대하는지, UNICODEMGR 1208 협상 시 UTF-8로 받는지 — derby는 전환, pydrda·jb는 cp500 고정. pydrda·jb 모두 실서버에서 동작한다고 표방하므로 **cp500이 안전한 기본**(추정 · 실서버로 확인 필요 — ASCII 글자도 cp500과 UTF-8의 바이트가 달라서 인코딩이 틀리면 SECCHKCD 실패로 바로 드러난다).
- 비밀번호 길이 상한·SECTKN 최대 길이(DDM 규정)는 미확인.
- FIPS 문서 절 이름 · 위 DES 벡터 값은 기억 기반(**추정**) — 구현 시 원문으로 대조.
