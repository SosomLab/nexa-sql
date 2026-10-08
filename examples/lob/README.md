# LOB 유형별 시험 SQL (DBMS별)

큰 값(LOB)을 **만들고 → 넣고 → 읽어 확인**하는 시험 스크립트다. 이미지(PNG · BMP), 한글이 섞인 큰 글(84만 자), 경계 길이(4,001자 · 32,768자), 특수 문자, 빈 값과 NULL, 1 MiB 이진을 넣는다. 끝의 **판정 표**에서 `RESULT` 열이 전부 `OK`면 넣은 그대로 읽힌 것이다. 그 뒤 전체 행을 다시 조회하므로 GUI 값 창(Text · Hex · Image)으로 눈으로도 본다. 설계는 [docs/87 §5](../../docs/87-grid-data-editing.md)(LOB 보기 · 값 창)를 따른다.

| 파일 | 다루는 타입 | 실행 | 서버에 남는 것 |
|---|---|---|---|
| [oracle.sql](oracle.sql) | BLOB · CLOB · NCLOB · XMLTYPE · LONG · LONG RAW(BFILE은 부록 주석) | `nsql run -c <프로필> examples/lob/oracle.sql` | 표 `NSQLT_LOB` · `NSQLT_LOB_LONG` · `NSQLT_LOB_LRAW` |
| [mssql.sql](mssql.sql) | VARBINARY(MAX) · VARCHAR(MAX) · NVARCHAR(MAX) · XML · 옛 IMAGE · TEXT · NTEXT | `nsql run -c <프로필> examples/lob/mssql.sql` — ⚠ 기본 DB가 master면 파일 위쪽 `USE` 줄을 시험용 DB로 고쳐 주석을 푼다 | 표 `dbo.NSQLT_LOB` · `dbo.NSQLT_LOB_LEGACY` |
| [pg.sql](pg.sql) | bytea · text · varchar · jsonb · xml · 큰 객체(oid · `lo_from_bytea`) | `nsql run -c <프로필> examples/lob/pg.sql` | 표 `public.nsqlt_lob` + 큰 객체 1개 |
| [sqlite.sql](sqlite.sql) | BLOB · TEXT(+ JSON 글) | `nsql run -c sqlite::memory: examples/lob/sqlite.sql` · 파일에 남기려면 `-c sqlite:lob.db` | 메모리 DB면 없음 |

- 표 이름은 시험용 접두 `NSQLT_`다. 각 파일의 **0절이 먼저 지우고 새로 만들기** 때문에 여러 번 돌려도 된다. 다 보고 나면 맨 끝 **정리** 절의 주석을 풀어 지운다. PostgreSQL은 큰 객체를 `lo_unlink`로 먼저 지운다.
- 운영 서버에서는 돌리지 않는다.
- MySQL/MariaDB는 드라이버가 없어 파일이 없다.

## 행 구성과 기대값 (네 파일 공통)

| ID | KIND | 넣는 것 | 판정 표가 보는 것 |
|---|---|---|---|
| 1 | IMAGE_PNG | 64×64 그라디언트 PNG(테두리 · 대각선) **7,141 bytes** | 길이 · 머리 `89504E470D0A1A0A` · 3,001번째 8바이트 `0EEE166FCB6E8BBF` · 꼬리 `49454E44AE426082` · MD5 `0b4bfa3bc61a836e93ee34a5c18f6d97`(SQL Server · PG) |
| 2 | IMAGE_BMP | 8×8 체크 BMP **246 bytes** | 길이 · 머리 `424DF6…` · 꼬리 `00FFFFC8000000FF` · MD5 `45d1ec3207d371a7d7cc75a3fdc6efcf` |
| 3 | TEXT_BIG | `LINE 000001 가나다라마바사 ABCDEFGHIJ 0123456789` … 20,000줄 = **840,000자**(UTF-8 1,120,000 bytes · UTF-16 1,680,000 bytes) · SQL Server VARCHAR / PG varchar = ASCII 판 900,000자 | 글자 수 · 첫 줄 · 10,000번째 줄 위치 419,959 · 마지막 줄(839,959부터 41자) |
| 4 | TEXT_4001 | `A` × 4,000 + `Z` | 길이 4001 · 마지막 글자 Z(Oracle VARCHAR2 · SQL Server NVARCHAR 4,000 경계) |
| 5 | TEXT_32768 | `B` × 32,767 + `Z` | 길이 32768 · 마지막 글자 Z(PL/SQL 32,767 · SQL Server VARCHAR 8,000 경계 넘음) |
| 6 | SPECIAL | 탭 · CR LF · 따옴표 · 백슬래시 · 한글 · 이모지 😀 + XML/JSON | XML/JSON 안의 값 `가나다` |
| 7 | EMPTY | 길이 0(EMPTY_BLOB · `0x` · `''`) | 길이 0 |
| 8 | NULLS | NULL | NULL — **7행과 다르게 보여야 한다** |
| 9 | BIN_1MB | `0123456789ABCDEF` 반복 **1,048,576 bytes**(이미지 아닌 큰 이진) | 길이 · 꼬리 8바이트 |
| 10 | (PG) LARGE_OBJECT | 1행 PNG를 큰 객체로 복사 | `lo_get` MD5 = 1행과 같음 |
| — | 옛 타입 | Oracle LONG(4,000자 · 32,000자) · LONG RAW(PNG · BMP) / SQL Server IMAGE · TEXT · NTEXT | SQL Server는 길이 판정 · Oracle LONG은 SQL 함수를 못 써서 보기만 |

## GUI에서 보기

1. 그 DBMS 접속 탭에서 파일을 열고 **전체 실행(F5)**. 판정 표 결과 탭의 `RESULT` 열이 전부 `OK`인지 본다.
2. 다음 결과 탭(전체 행)에서 LOB 셀을 **더블클릭**하면 값 창이 열린다.
   - 1·2행 `B` = **Image** 보기: 64×64 그라디언트와 대각선, 8×8 체크.
   - **Hex** 보기: 머리가 `89 50 4E 47` / `42 4D`.
   - 3행 = **Text** 보기: Ctrl+F로 `LINE 010000`을 찾고 끝까지 스크롤한다.
   - 9행 = Hex 보기. 큰 값은 앞부분만 보이고 안내가 뜬다(`grid.lob_view_max_mb`).
3. 값 창 **[파일로 저장]**으로 저장한 PNG/BMP의 MD5가 위 표와 같은지 본다. PowerShell `Get-FileHash -Algorithm MD5 <파일>`.
4. 값 창 **[파일에서 넣기]**로 다른 그림을 넣고 ✓ 적용한 뒤, 다시 조회해 길이와 머리를 확인한다(그리드 편집 · 87 §5-1).

## 확인 기록

**2026-10-08**(협업 세션 · nsql Release CLI) 결과:

| DBMS | 서버 | 판정 표 | 걸린 시간 |
|---|---|---|---|
| Oracle | 19c · AL32UTF8 | 20/20 OK | 5.8 s |
| SQL Server | 2019 · `USE M4PLAN_MS` | 22/22 OK | — |
| PostgreSQL | Repository | 22/22 OK | — |
| SQLite | 3.46 | 19/19 OK | 0.16 s |

- GUI 값 창(SQLite 파일 DB · Debug · 기동 명령):
  - PNG = `mode=Image kind=PNG 64x64`
  - BMP = `kind=BMP 8x8`
  - 1 MiB = `mode=Hex bytes=1048576`
  - [파일로 저장]한 두 파일의 MD5가 원본과 같았다.
- 시험 뒤 서버의 표와 큰 객체는 지웠다.
