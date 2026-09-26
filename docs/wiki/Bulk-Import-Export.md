# 파일로 넣기 · 내보내기 (대량 적재)

CSV · TSV · JSON Lines 파일을 표에 **넣고**(import), 조회 결과를 파일로 **내보냅니다**(export). GUI와 CLI가 같은 코드를 쓰므로 결과와 안내 문구가 같습니다.

> 넣기는 **INSERT만** 합니다. 기존 행을 고치거나 지우지 않습니다(UPDATE·DELETE는 [결과 그리드에서 데이터 편집](Data-Editing.md)의 안전 규칙을 따릅니다).

## 1. 화면에서 넣기 (Import 창)

1. 탐색기에서 **표**를 우클릭 ▸ **Import Data…**
2. 파일을 고릅니다(csv · tsv · jsonl · ndjson).
3. Import 창에서 미리보기와 옵션을 확인하고 **Start**.

| 창의 항목 | 뜻 |
|---|---|
| 맨 윗줄 | 대상 표와 파일 경로(길면 가운데를 줄여 보여 줍니다) |
| **First row = column names** | 첫 줄이 열 이름인지(끄면 표의 열 순서대로 채웁니다) |
| **Map (src=dst,…)** | 파일의 열 이름을 표의 열 이름으로 바꿔 붙입니다. 예: `emp_no=EMPNO,name=ENAME` |
| **Batch** | 한 번에 보내는 행 수(왕복 단위) |
| **Commit every** | 몇 행마다 커밋할지(0 = 끝에 한 번) |
| **Path** | 적재 경로 — 누르면 `auto → driver → multirow → single`로 돌아갑니다(아래 §3) |
| 미리보기 | 파일 앞 6줄(읽기 전용) |
| 아랫줄 | 진행 중에는 커밋된 행 수와 경과, 끝나면 결과(성공 = 초록 · 실패·취소 = 빨강) |

- 적재는 **지금 탭의 연결**로 실행됩니다. 탐색기에서 고른 표가 다른 서버라면 그 탭을 먼저 그 서버에 접속하라고 안내합니다.
- 적재 중에는 그 연결이 바쁜 상태가 되고(다른 실행은 기다립니다), **Cancel**은 다음 배치 경계에서 멈춥니다. 이미 커밋된 행은 그대로 남고 커밋되지 않은 마지막 배치만 되돌립니다.
- 창을 닫아도(Esc · Close) 진행 중이면 먼저 취소가 됩니다.

## 2. 명령으로 넣기 (`nsql import`)

```
nsql import -c <대상> -t <표> [옵션] <파일|->
```

```
nsql import -c prod -t EMP emp.csv
nsql import -c prod -t EMP -f tsv --map emp_no=EMPNO,name=ENAME --commit-every 5000 emp.tsv
cat rows.jsonl | nsql import -c prod -t EMP -
```

| 옵션 | 뜻 |
|---|---|
| `-f csv\|tsv\|jsonl` | 형식을 직접 지정(생략하면 확장자로 판단: `.tsv`·`.tab` = 탭, `.jsonl`·`.ndjson` = JSON Lines, 그 밖 = 쉼표) |
| `--no-header` | 첫 줄도 데이터(열은 표의 순서대로) |
| `--cols a,b,…` | 파일의 열 이름을 순서대로 지정(헤더가 없을 때) |
| `--map src=dst,…` | 파일 열 → 표 열 이름 바꾸기 |
| `--batch N` | 한 번에 보낼 행 수 |
| `--commit-every N` | N행마다 커밋(0 = 끝에 한 번) |
| `--mode auto\|driver\|multirow\|single` | 적재 경로(§3) |
| `--empty-null on\|off` | 빈 칸을 NULL로 볼지, 빈 문자열로 둘지 |
| `--timing` | 단계별 시간을 stderr로 |

- 파일 이름 자리에 `-`를 주면 표준 입력에서 읽습니다.
- 값이 큰따옴표로 감싸여 있으면 그 안의 쉼표·줄바꿈을 그대로 읽습니다. 파일 앞의 BOM은 무시합니다.
- **JSON Lines**는 한 줄에 객체 하나입니다. 첫 객체의 키가 열 이름이 되고, 어떤 줄에 없는 키나 `null`은 NULL로 들어갑니다.
- 날짜·숫자는 표의 열 타입에 맞춰 보냅니다. 형식이 맞지 않으면 그 행에서 서버 오류로 멈추고 어느 행인지 알려 줍니다.

## 3. 적재 경로와 속도

`auto`(기본)는 DBMS가 제공하는 빠른 길이 있으면 그것을 쓰고, 없으면 여러 행을 한 문장에 담아 보냅니다.

| DBMS | 자동으로 고르는 길 |
|---|---|
| Oracle | 배열 DML(한 번에 배치 크기만큼 바인드) |
| PostgreSQL | `COPY … FROM STDIN` |
| SQL Server | TDS 벌크 적재 |
| SQLite · 그 밖 | 다중 행 `INSERT` |

측정값(5,000행 · 4열 · 원격 서버 · 디버그 빌드) — 환경에 따라 달라집니다.

| DBMS | 초당 행 |
|---|---|
| SQLite | 약 57,000 |
| PostgreSQL | 15,000 ~ 20,000 |
| Oracle | 13,000 ~ 14,000 |
| SQL Server | 5,200(배치 1,000) → 13,900(배치 5,000) |

- SQL Server는 배치마다 준비 왕복이 있어 `--batch`를 키울수록 빨라집니다.
- `driver`는 빠른 길이 없으면 오류로 알려 주고, `multirow`는 항상 다중 행 INSERT, `single`은 한 행씩 보냅니다(문제를 찾을 때).

## 4. 실패했을 때

- 배치가 실패하면 그 배치를 **한 행씩 다시 넣어** 문제 행을 지목합니다: `row 500 (line 501)` 처럼 **파일의 몇 번째 행 · 몇 번째 줄**인지와 서버 오류 메시지를 보여 줍니다.
- 그 앞까지 성공한 행은 커밋되고 몇 행이 들어갔는지 함께 알려 줍니다(`499 rows committed`).
- 전부-아니면-무로 하고 싶으면 **커밋 간격을 0**으로 두세요(`--commit-every 0` · 창의 Commit every = 0). 대신 큰 파일에서는 서버의 트랜잭션이 길어집니다.

## 5. 내보내기 (`nsql export`)

```
nsql export -c <대상> (-q <SQL> | -t <표>) [-f csv|tsv|json|jsonl|insert[:표]] [-o 파일] [--fast]
```

`--fast`는 **서버가 직접 csv/tsv를 만들어** 보내게 합니다(PostgreSQL `COPY TO`). 지원하지 않는 DBMS·형식이면 한 줄 안내 뒤 평소 경로로 내보냅니다.

## 6. 설정 (`bulk.*`)

창과 명령의 기본값입니다(설정 ▸ 검색창에 `bulk`).

| 키 | 기본 | 뜻 |
|---|---|---|
| `bulk.batch_rows` | 1000 | 한 번에 보낼 행 수 |
| `bulk.commit_every` | 10000 | 커밋 간격(0 = 끝에 한 번) |
| `bulk.mode` | auto | 적재 경로 |
| `bulk.empty_null` | on | 빈 칸 = NULL |
| `bulk.check_constraints` | on | SQL Server 벌크 적재 중 CHECK·FK 검사 유지 |
| `bulk.fire_triggers` | off | SQL Server 벌크 적재 중 INSERT 트리거 실행 |
| `bulk.append_hint` | off | Oracle 직접 경로 삽입(`/*+ APPEND */`) — 빠르지만 표를 잠그고 되돌리기 특성이 달라집니다 |
