# 127. 데이터 가져오기(Import) 설계 — 파일 → 표 (xlsx · parquet · json · csv/tsv/jsonl 한 경로)

> **상태**: 📐 설계(11-04 새벽 · 개발 세션 · 사용자 11-04 "export 뒤 import도 — DataGrip 등 최신 프로그램을 분석해 사용자 편의성과 속도가 좋은 방향") → 구현 단위 §7 순서로 commit·push.
> **요청 원문**(사용자): "import의 방식 및 타입 지정 등은 DataGrid 등 최신 프로그램을 분석해서 사용자 편의성과 속도가 좋은 방향으로 개발" · 앞선 결정 = D-446(T-8 가져오기 = parquet + xlsx + "TSV·CSV 포함" = 기존 csv/tsv/jsonl과 **한 경로** · xlsx 읽기 crate = DR-3 예외 원장 10 §4 추가 · 개발 선정 뒤 알림).
> 관련 = [89 대량 I/O](89-bulk-io-review.md)(적재 경로 = 드라이버 최속 경로 · 다중 행 INSERT · 실패 지목 · 취소) · [40 Step 12](40-cli-usage.md) · [43](43-fetch-model-and-result-tabs.md) · [61 §1-7 데이터 보호](61-core-design-and-working-rules.md) · 내보내기 짝 = D-448([journal 11-04 §0](journal/2026-11-04.md)).

## 0. 한 줄 결론

**형식 6(csv · tsv · json · jsonl · xlsx · parquet)을 읽기 포트 하나(`RecordSource`)로 묶고 · 앞 N행을 보고 형을 추론해 "기존 표에 넣기 / 새 표 만들기"를 **한 창**에서 미리보기 그리드 + 열 매핑 표로 고치게 한 뒤 · 적재는 지금 있는 최속 경로(`Runner::bulk_load` · 드라이버 싱크 → 다중 행 INSERT)에 그대로 태운다.** 쓰기는 INSERT(+ 새 표 CREATE)뿐 — UPDATE·DELETE·TRUNCATE를 자동으로 만들지 않는다(61 §1-7 정신 · 비우기는 사용자가 직접).

## 1. 조사 — 최신 도구가 하는 것(11-04 · 공식 문서 우선)

| 도구 | 원료 | 대상 | 형 결정 | 편의 요소 | 속도·안전 | 출처 |
|---|---|---|---|---|---|---|
| **DataGrip 2024.3** | CSV/TSV(구분자 · 인용 · 이스케이프 · 첫 줄 헤더 · 미리보기 행 우클릭 "이 줄을 헤더로") · 클립보드 | 기존 표 또는 **새 표**(DDL 미리보기) | 매핑 트리에 열마다 **Type**(원료 열의 추정 형 · 편집 가능) · "변환 못 하는 값은 NULL로" 옵션 | Data Preview + DDL Preview 한 창 · 원료 열 ↔ 대상 열 짝 맞춤 | 백그라운드 · 오류 행 건너뛰기 선택 | jetbrains.com/help/datagrip/2024.3/import-data.html |
| **DBeaver 25.x** | CSV(인코딩 · 구분자 · 헤더 위치 · 인용) · **XLSX**(시트 선택 · 모든 시트 → 여러 표 · 날짜 형식) · XML | 기존/새 표 · 매핑 단계(Ultimate) | "**형 추정에 읽을 행 수**" 설정 | 마법사 5단(원료 → 설정 → 매핑 → 적재 옵션 → 확인) | 백그라운드 · 적재 옵션이 성능에 영향(배치 · 커밋) | dbeaver.com/docs/dbeaver/Data-import · 24.2 Data-transfer |
| **Beekeeper Studio** | CSV · JSON · JSONL · XLSX | 기존 표 또는 **파일에서 새 표** | 열 이름·형을 매핑 단계에서 정의 · 자동 형식 감지 | **앞 10행 미리보기(설정 적용 상태)** · 자동 열 매핑 | **트랜잭션 하나 · 실패 = 전부 롤백(새 표는 제거)** | docs.beekeeperstudio.io/user_guide/importing-data-csv-json-etc |
| DbVisualizer Pro | CSV · Excel | 기존/새 표 | 형 추정 옵션 | 미리보기 그리드 + 파일 보기 | 단건 vs 배치 INSERT · 커밋 간격 | dbvis.com(2026 비교 글 · 2차 출처) |

**공통 패턴**(우리가 따를 것) = ① 형식·헤더·구분자는 **자동 감지 + 바로 바꿀 수 있는 콤보** ② **앞 N행 미리보기가 설정을 바꾸는 즉시 따라감** ③ 대상 = 기존 표(열 짝 맞춤 자동 · 건너뛰기) / 새 표(추론 형 · 이름·형 편집 · DDL 미리보기) ④ 적재는 배치·커밋 간격·백그라운드·취소 ⑤ 실패 = 행 지목.
**우리만의 차이** = 적재 엔진이 이미 있다(89 §3-4-a · Oracle 배열 DML 32k행/s · PG COPY · TDS bulk) → 이번 일은 **읽기 층 + 형 추론 + 새 표 + UI**다.

## 2. 사용자 면(설계)

### 2-1. 진입점 3
1. 탐색기 **표** 우클릭 ▸ Import Data… → 파일 창 → Import 창(대상 = 그 표 · 기존 표 모드).
2. 탐색기 **스키마/접속** 우클릭 ▸ Import Data… → 파일 창 → Import 창(대상 = **새 표** · 이름 = 파일 줄기(xlsx = 시트 이름) 정리값).
3. 결과 그리드 우클릭 ▸ 파일로 내보내기와 짝 = 메뉴 **Database ▸ Import Data…**(현재 세션 · 새 표 기본).

### 2-2. Import 창 = 한 창 · 세 구역(위 → 아래 · 960×680 · 크기 조절)

```
┌ Nexa SQL - Import Data - HR.EMP ───────────────────────────────────────────────────────────┐
│ 파일  C:\…\sales_2026.xlsx                      형식 [Excel 통합 문서 ▾]  시트 [Sheet1 ▾]    │ ← A 원료
│ [✓] 첫 줄 = 열 이름   구분자 [, ▾] 인용 ["] 인코딩 [자동(UTF-8) ▾]  [ ] 공백 다듬기  NULL 글자 [   ] │   (형식별로 보이는 칸만)
├─ 미리보기(앞 100행 · 추론 형) ──────────────────────────────────────────────────────────────┤
│ id · INT │ name · TEXT(12) │ hired · DATE │ sal · DECIMAL(10,2) │ …                          │ ← B 결과 그리드 재사용
│ 1        │ Kim             │ 2024-03-01   │ 3500.00             │                              │   (헤더 = 이름 · 형)
├─ 대상 ──────────────────────────────────────────────────────────────────────────────────────┤
│ (●) 기존 표 [HR.EMP        ]   ( ) 새 표 [SALES_2026      ]   [DDL 보기]                     │ ← C 대상·매핑
│ ✓ │ 원료 열 │ 추론 형        │ → 대상 열 ▾      │ 형(새 표) ▾     │ 표본                       │
│ ✓ │ id      │ INT            │ EMP_NO           │ NUMBER(10)      │ 1, 2, 3                    │
│ ✓ │ name    │ TEXT(12)       │ ENAME            │ VARCHAR2(40)    │ Kim, Lee                   │
│ ☐ │ memo    │ TEXT(200)      │ — 건너뜀 —       │                 │                            │
│ 배치 [1000] 커밋 간격 [10000] 경로 [auto ▾] [✓] 빈 값 = NULL  오류 시 (●) 멈춤 ( ) 행 건너뛰고 보고 │
│ 진행: 12,000행 · 3.2초 · 3,750행/s                            [시작] [취소] [닫기]              │
└────────────────────────────────────────────────────────────────────────────────────────────┘
```

- **A 원료** = 형식 콤보(확장자로 자동 · 바꿀 수 있음 · 7 = csv/tsv/json/jsonl/xlsx/parquet/자동) · 형식별 칸만 보임(csv/tsv = 헤더·구분자·인용·인코딩 · xlsx = 시트·헤더 · json/jsonl = 없음 · parquet = 없음) · 구분자는 앞 64 KB로 자동 감지(`,` `;` `\t` `|`) · 인코딩 자동 = BOM → UTF-8 검증 → **cp949(EUC-KR)**(한국 Windows Excel이 저장한 CSV · 사용자 편의 1순위) · 바꾸면 **B·C가 즉시 다시 계산**(스레드 · 앞 N행만 읽어 200 ms 안).
- **B 미리보기** = 결과 그리드 컨트롤 재사용(`grid::Grid` · 읽기 전용 · 100행 · 헤더에 `이름 · 추론 형`) — 새 컨트롤을 만들지 않는다(30 §2 · DR-33 `ResultData`).
- **C 대상** = 라디오(기존 표 / 새 표) · 매핑 표(열마다 ✓ 포함 · 대상 열 콤보(기존 표 열 목록 + "— 건너뜀 —") · 새 표면 형 콤보(방언 형 목록 · 추론값 기본) · 표본 값) · 자동 짝 맞춤 = 이름(대소문자·`_`·공백 무시) → 남은 것은 **위치** 순 · 적재 옵션 = 종전 Import 창 그대로(설정 `bulk.*` 기본) + **오류 시**(멈춤(기본) / 행 건너뛰고 보고 = DataGrip "NULL로"와 다르게 **행 단위 건너뜀**(값 하나만 NULL로 바꿔 넣는 건 데이터를 조용히 바꾸는 일이라 하지 않는다 · 61 §1-7 정신)).
- **DDL 보기** = 새 표 모드에서 생성할 `CREATE TABLE` 전문(SQL Preview 창 재사용 · 복사 가능 · 수정은 매핑 표에서).
- **시작** = (새 표면 CREATE 먼저 · 실패 = 아무것도 넣지 않음) → `Runner::bulk_load`(종전) · 진행 = 행·초·행/s · 취소 = 배치 경계 롤백 · 끝 = 요약 + 실패 지목(행·줄) + **새 표면 탐색기 새로 고침 + "SELECT * 열기" 단추**.
- 설정(새 접두 `import.` · 카테고리 CLI ▸ 대량 I/O와 같은 자리 = `bulk.` 옆 · 94 원칙) = `import.sample_rows`(형 추론 표본 · 1000 · 0 = 전부) · `import.preview_rows`(100) · `import.encoding`(auto) · `import.on_error`(stop/skip) · `import.text_margin_pct`(새 표 문자열 길이 여유 · 50 → `TEXT(12)` → `VARCHAR(18)`) · 기존 `bulk.*` 그대로.

### 2-3. CLI(`nsql import` 확장 · 40 Step 12)

```
nsql import -c <연결> -t <표> [-f csv|tsv|json|jsonl|xlsx|parquet] [--sheet NAME|N] [--delim ';'] [--quote '"'] [--encoding auto|utf-8|euc-kr|utf-16le]
            [--create] [--types a:number(10),b:varchar2(40)] [--skip memo,etc] [--sample N] [--on-error stop|skip]
            [--dry-run]   <파일>
```
- `--create` = 새 표(추론 형 · `--types`로 일부 덮기) · `--dry-run` = 추론 결과(열 · 형 · 표본) + DDL + 앞 10행을 stdout에 찍고 끝(접속은 방언 판별에만) · 그 밖은 종전 옵션 그대로.
- JSON(배열 파일)은 전체를 읽는다(크기 > `import.json_max_mb` 256이면 JSON Lines로 바꾸라는 안내 · jsonl은 스트리밍).

## 3. 읽기 층(nsql-io · 의존 격리)

```rust
/// 원료 한 레코드 = 셀 글(없음 = NULL) · 원료 행 번호(1부터) · 줄 번호(글 파일만 · 이진은 행 번호와 같음)
pub struct Record { pub cells: Vec<Option<String>>, pub row: u64, pub line: u64 }
pub trait RecordSource {
    /// 열 이름(헤더 · 없으면 `col1..`) — 첫 호출에서 머리를 읽는다.
    fn columns(&mut self) -> io::Result<Vec<String>>;
    fn next(&mut self) -> io::Result<Option<Record>>;
    /// 전체 행 수를 미리 알면(parquet 메타 · xlsx 범위) 진행률에 쓴다.
    fn total_hint(&self) -> Option<u64> { None }
}
pub fn open_source(path: &Path, opts: &SourceOpts) -> io::Result<Box<dyn RecordSource>>;   // 형식 자동(확장자 → 내용 스니핑)
```

| 형식 | 구현 | 외부 crate | 메모 |
|---|---|---|---|
| csv/tsv | 기존 `delim::DelimReader` + **인코딩 디코더 래퍼**(`DecodeReader<R: Read>` · `encoding_rs::Decoder` 스트리밍 · BOM) + 구분자·인용 옵션 | `encoding_rs`(nexa-sql `enc.rs`가 이미 쓰는 DR-3 예외 · nsql-io로 확장 등재) | 자동 구분자 = 앞 64 KB에서 후보 4의 줄당 개수 분산이 가장 작은 것 |
| jsonl | 기존 `jsonl::JsonlReader` | 없음 | 키 = 첫 객체 ∪ 뒤 객체의 새 키(표본 단계에서 수집) |
| json | `nsql_json::parse` 전체 → 배열의 객체 → 레코드 · 스칼라 배열은 `value` 한 열 | 없음 | 크기 상한 안내 |
| **xlsx** | `xlsx_in.rs`(feature `import-xlsx` · 기본 켬) — `calamine::open_workbook_auto` → 시트 목록 · `worksheet_range` → 셀 형(Int·Float·String·Bool·DateTime(시리얼 → ISO 날짜/일시 직접 환산 · chrono 없음)·Empty·Error) → 글 | **`calamine` 0.38**(순수 Rust · xlsx/xls/xlsb/ods) · **DR-3 예외 원장 10 §4 추가(D-449)** | 시트 범위를 메모리에 올림(xlsx 상한 1,048,576행 · 셀 수 비례) — 진행률 = 행/총행 |
| parquet | `parquet_in.rs`(feature `parquet` = 기존 `export-parquet` crate 공유) — `SerializedFileReader` 행 그룹 순회 · `Row`/`Field` → 글(Decimal 자리수 복원 · Date/Timestamp ISO) | 기존 `parquet`(arrow 없음) | 스트리밍(행 그룹 단위) · `total_hint` = 메타 행 수 |

## 4. 형 추론(`nsql_io::infer` · 순수 · 시험)

- 입력 = 앞 `sample_rows`행의 셀 글(xlsx/parquet는 셀 형이 있으면 그 힌트 우선) → 열마다 **가장 좁은 형**:
  `Bool`(true/false/y/n/0/1 전부일 때만) ⊂ `Int`(i64 · 선행 0은 글) ⊂ `Decimal(p,s)` ⊂ `Float`(지수 표기) · `Date`(ISO · `yyyy/MM/dd` · `yyyyMMdd` 8자리는 선택 끔) · `DateTime` · `Time` · `Text(max_chars)` · 빈 셀 = NULL(nullable 표시 · 형 판정에서 제외) · 전부 빈 열 = `Text(1)`.
- **방언 형 이름 표**(새 표 DDL · 사용자가 콤보로 바꿀 수 있는 후보 목록과 같은 표):

| 추론 | Oracle | PostgreSQL | SQL Server | SQLite | MySQL/MariaDB | Db2 |
|---|---|---|---|---|---|---|
| Bool | NUMBER(1) | boolean | bit | INTEGER | TINYINT(1) | BOOLEAN |
| Int | NUMBER(19) | bigint | bigint | INTEGER | BIGINT | BIGINT |
| Decimal(p,s) | NUMBER(p,s) | numeric(p,s) | decimal(p,s) | NUMERIC | DECIMAL(p,s) | DECIMAL(p,s) |
| Float | BINARY_DOUBLE | double precision | float | REAL | DOUBLE | DOUBLE |
| Date | DATE | date | date | TEXT | DATE | DATE |
| DateTime | TIMESTAMP | timestamp | datetime2 | TEXT | DATETIME | TIMESTAMP |
| Time | VARCHAR2(16) | time | time | TEXT | TIME | TIME |
| Text(n) | VARCHAR2(n′) · n′>4000 = CLOB | varchar(n′) · >10485760 = text | nvarchar(n′) · >4000 = nvarchar(max) | TEXT | VARCHAR(n′) · >16383 = TEXT | VARCHAR(n′) · >32672 = CLOB |

  (n′ = n × (1 + `import.text_margin_pct`/100) 올림 · 최소 1 · 한글은 글자 수 기준이라 Oracle BYTE 의미 집합이면 ×3 — 세션 `NLS_LENGTH_SEMANTICS` 조회는 후속 · 1차 = CHAR 명시 `VARCHAR2(n CHAR)`.)
- 새 표 이름·열 이름 = 방언 식별자 규칙(`identq::needs_quote` · 예약어 표 D-447 · 공백/특수문자 → `_` · 빈 이름 → `col3`) · 중복 이름 → `_2`.

## 5. 적재(nsql-run · 종전 엔진 위에)

- `ImportSpec` 확장 = `source: SourceOpts{format, delim, quote, header, encoding, sheet, trim, null_text}` · `skip: Vec<String>` · `create: Option<CreateSpec{table, cols: Vec<(name, type)>}>` · `on_error: Stop|Skip` · `sample_rows` (종전 필드 = 그대로 · CLI·GUI 공용).
- `Runner::import_inspect(path, &spec) -> Inspect{columns, kinds, nullable, preview: ResultSet, total_hint, sheets}` = GUI 미리보기·CLI `--dry-run`·새 표 DDL의 입력(표본만 읽음 · 200 ms 목표 · 스레드).
- `Runner::import_file` = `open_source` → (create면 `CREATE TABLE` 1문 · 실패 = 중단) → 열 매핑(이름 → 위치) → `bulk_load`(종전) · `on_error = Skip` = 실패한 배치를 **한 행씩 재시도**해 실패 행을 건너뛰고 `BulkReport.skipped: Vec<BulkFailure>`(상한 1000 · 끝에 보고서 창/stderr) — 종전 "실패 배치 한 행씩 재실행해 지목"하는 길을 그대로 이어 쓴다.
- 안전 = 쓰기는 INSERT와 (사용자가 고른) CREATE뿐 · 기존 표 비우기/덮어쓰기 옵션 없음(필요하면 편집기에서 직접) · 취소 = 배치 경계 롤백 · 커밋 간격 0 = 끝에 한 번(= Beekeeper식 전부-아니면-전무).

## 6. 성능 목표·측정(71 · 26 §8 · 39 §3)

- 미리보기/추론 = 표본 1000행 · 파일 크기와 무관 · ≤ 200 ms(xlsx는 시트 적재 비용 = 첫 열기 1회 · 100 MB 시트 수 초 → 진행 표시).
- 적재 = 종전 경로 그대로(SQLite 다중 행 · Oracle 배열 DML · PG COPY · TDS bulk) · 읽기 층 오버헤드 ≤ 10 %(csv 디코더 스트리밍 · parquet 행 그룹) · 측정 자산 = `scripts/import-bench.sh`(sqlite · 100만 행 csv/xlsx/parquet 생성 → 적재 행/s · 메모리 피크) · 부하원 등재(39 §3) = 추론 스레드(`import.sample_rows` 상한) · JSON 전체 적재(`import.json_max_mb`).

## 7. 구현 단위(각각 commit · push · 기능 단위 시험)

| # | 단위 | 크레이트 | 검증 |
|---|---|---|---|
| ① | 읽기 포트 `RecordSource` + csv/tsv(인코딩·구분자 자동) + jsonl + json + `open_source` | nsql-io(+`encoding_rs`) | 단위 시험(BOM · cp949 · 구분자 감지 · 인용 · json 배열) |
| ② | xlsx(`calamine` · D-449) + parquet 읽기 | nsql-io | 왕복 시험(내보내기 xlsx/parquet → 가져오기 → 같은 글) |
| ③ | 형 추론 + 방언 형 표 + 새 표 DDL 생성 | nsql-io `infer` · nsql-run | 표 시험(좁은 형 승격 · 날짜 · 길이 여유 · 식별자 인용) |
| ④ | `ImportSpec` 확장 · `import_inspect` · `import_file` 일반화(create · skip · on_error) · 설정 `import.*` | nsql-run · nsql-settings · nsql-i18n | sqlite 통합(기존 표 · 새 표 · 오류 건너뛰기 · 취소) |
| ⑤ | CLI 플래그 + `--dry-run` + 40 Step 12 문서 | nsql-cli | 명령 예 실행 캡처 |
| ⑥ | GUI Import 창 재설계(A·B·C) + 진입점 3 + 기동 명령(`import.open:` 확장 · `import.set:<키>=<값>` · `import.dump:`) | nexa-sql | 헤드리스 E2E(격리 홈 · sqlite · csv/xlsx 새 표 · 기존 표 매핑 · 취소) |
| ⑦ | 측정 `import-bench.sh` · 39/26 등재 · 문서 트랜잭션 | scripts · docs | 수치 표 |

## 8. 결정(추천안 = 진행 · 사용자 1분 무응답 규칙)

| # | 결정 | 추천(진행) | 대안 · 이유 |
|---|---|---|---|
| D-449 | xlsx 읽기 crate | **`calamine`**(순수 Rust · 활발 · xls/ods 덤) · DR-3 예외 원장 등재 | 직접 zip+xml 파서(수 주) · `umya-spreadsheet`(쓰기 중심 · 무거움) |
| D-450 | 새 표 생성 | **포함**(세 도구 전부 있음 · INSERT/CREATE만이라 데이터 보호 불변식과 무관) | 기존 표만(불편 · 사용자 "편의성") |
| D-451 | 기존 표 비우기(TRUNCATE/DELETE) 옵션 | **없음**(자동 생성 삭제문 금지 정신 · 편집기에서 직접) | 체크박스(실수 한 번이 표 전체 삭제) |
| D-452 | 변환 실패 처리 | **행 단위 건너뛰고 보고**(선택) · 기본 = 멈춤 | DataGrip "값을 NULL로"(데이터를 조용히 바꿈 · 채택 안 함) |
| D-453 | 미리보기 컨트롤 | **결과 그리드 재사용**(`grid::Grid` 읽기 전용) | 새 표 컨트롤(규칙 위반 · 중복) |
| D-454 | 인코딩 기본 | **자동(BOM → UTF-8 → cp949)** · 설정 `import.encoding` | UTF-8 고정(한국 Excel CSV 깨짐) |
