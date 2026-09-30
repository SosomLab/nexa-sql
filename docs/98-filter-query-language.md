# 98. 필터 질의 언어 — 용량·종류 술어 + 논리 조합 (2026-09-30)

> 사용자 09-30: "객체 필터에서 용량을 기준으로 검색 · 기본 검색은 문자열 · `Size>1G` 같은 형태 · 최근 사용자가 많고 편의성이 좋다고 평가된
> 도구에서 기법을 스터디 · 논리 조합 지원 · 객체 탐색기에 적용되면 다른 탐색기나 북마크에서도 · 용량은 객체 탐색기와 프로젝트 탐색기에서만 ·
> 기능 구현 및 지원 범위(제약) 결정."
>
> 구현 = nsql-core `filterq`(파서·평가기 · 의존 0) → nexa-sql `filterbar::Matcher`(모든 필터 상자의 판정기) → 각 패널이 `Facts`로 이름·종류·용량을 준다.
> 사용자 안내 = 위키 [Explorers-and-Filters](wiki/Explorers-and-Filters.md) "필터에 조건 쓰기". 부품 원장 = [30 §2](30-architecture-patterns.md) FilterQuery.

## 1. 스터디 — 널리 쓰이는 도구의 검색 문법

| 도구 | 술어 | 논리 | 용량 | 특징 |
|---|---|---|---|---|
| **Everything**(voidtools) | `size:>1gb` · `size:1mb..10mb` · `ext:sql` · `dm:` | 공백 AND · `\|` OR · `!` NOT · 괄호 · `""` | K/M/G 단위 · 범위 `..` | 가장 빠르고 널리 쓰이는 파일 검색 · 술어 = `키:값` |
| **Windows 검색(AQS)** | `size:>1GB` · `size:gigantic` · `kind:` · `date:` | `AND` `OR` `NOT` · 괄호 | 단위 + 이름 크기 | 탐색기 검색 상자 · 낱말 키워드 |
| **GitHub 검색** | `size:>1000` · `path:` · `language:` | `AND` `OR` `NOT` · `-` · 괄호 | 바이트 | 코드 검색 · 술어와 낱말 섞기 |
| **Gmail** | `larger:10M` · `smaller:` · `from:` | `OR` · `-` · 괄호 · `""` | K/M | 일반 사용자에게 익숙한 문법 |
| **Spotlight(원시 질의)** | `kMDItemFSSize > 1000000` | `&&` `\|\|` | 바이트 | 전문가용 · 채택 안 함 |
| DBeaver 객체 필터 | 이름 마스크 `*` 포함/제외 | 없음 | 없음 | 논리·용량 없음 |
| VS Code 뷰 필터 | `@ext:` `@tag:` 범위 토큰 | 없음 | 없음 | 낱말만 |

**공통 관례**(채택): 술어 = `키:값` / `키>값` / `키:>값`(둘 다) / 범위 `키:a..b` · 공백 = AND · `|` 또는 `OR` = OR · `!` 또는 `NOT` = NOT · 괄호 · `"구절"` · 용량 단위 K/M/G(T/P).
**채택하지 않음**: `-` = NOT(GitHub·Gmail) — 객체·파일 이름에 `-`가 흔해(`tb-order` · `my-file.sql`) 낱말로 본다. `&&`/`||`(Spotlight) — 일반 사용자 문법이 아니다.

## 2. 문법(`nsql-core::filterq`)

```
query := or
or    := and ( ('|' | OR) and )*
and   := not ( (AND)? not )*           -- 공백 = AND
not   := ('!' | NOT) not | atom
atom  := '(' or ')' | "구절" | 술어 | 낱말
술어  := 키 ( ':' | '>' | '>=' | '<' | '<=' | '=' | '!=' | ':>' … ) 값 | 키 ':' 값 '..' 값
```

- **낱말** = 이름 부분 일치(대소문자 무시 · 한글은 자모열 — 조합 중 "ㅈㅁ"도 "주문"에 걸린다).
- **키**: `size`(= `bytes` `len`) · `type`(= `kind`) · `name` · 그 밖(`ext` `path` …)은 패널이 주면 듣는다.
- **용량 값**: 숫자 + 단위 `B K M G T P`(`KB` `MB` `GiB`도) · 소수 허용(`1.5G`) · 단위 없음 = 바이트 · `size:1G` 하나만 쓰면 **이상(≥)** · `size:100M..1G` 양끝 포함.
- **종류 값**: `table` `view` `materializedview` `procedure` `function` `package` `packagebody` `sequence` `trigger` `index` `synonym` `column` `schema` · 파일 = `file` `folder` · **접두 일치**(`type:tab`) + 별칭(`mv` `matview` · `proc` `sp` · `func` · `pkg` · `body` · `seq` · `trg` · `idx` · `syn` · `col` · `dir`).
- **구조 판정**: 글에 `|` `(` `)` `"` · `!낱말` · `OR`/`AND`/`NOT` · `키:`/`키>` … 가 있을 때만 질의로 본다(`is_structured`). 낱말만이면 종전의 빠른 낱말 판정(캐시) 그대로 — 성능 회귀 0.
- **정규식 모드**(`(.*)`): 정규식이 전부 · 질의 없음.
- **깨진 입력**은 실패하지 않는다(괄호 미완 · 값 없는 술어 = 있는 데까지 · 낱말로).

## 3. 평가 규칙 · 제약(결정)

| 규칙 | 결정 | 이유 |
|---|---|---|
| 값이 없는 사실에 대한 술어 | **거짓**(제외) | "용량 미상"을 걸러 주는 편이 `size>1G`의 뜻에 맞다 · 알 수 없는 것을 포함하면 결과가 부풀어 오른다 |
| 객체 탐색기 `size` | **읽힌 객체만**(폴더 펼침 또는 우클릭 **용량 확인**으로 채운 테이블·MV·인덱스) · 스키마 행 = 읽힌 테이블·MV·인덱스 합 · 폴더 행 = 그 종류 합 | 지연 로딩 원칙(61 §1-8) — 필터가 전 스키마의 용량 질의를 유발하지 않는다 |
| 인덱스 검색(이름 인덱스 · 스레드) 결과 | `type`은 듣고 `size`는 미상 = 제외 | 인덱스에는 이름·종류만 있다(84 §2) |
| 프로젝트 탐색기 `size` | 파일만(`metadata` · **`size` 술어가 있을 때만** 읽는다) · 폴더 = 미상 | 키 입력마다 전 파일 stat 금지 — 술어가 있을 때 한 번씩 |
| 북마크 · 아웃라인 · 확장 · 파일 검색 결과 | 낱말 · 논리 조합 · `name:`만 | 용량·종류 사실이 없다(요구: 용량은 객체·프로젝트 탐색기만) |
| `-` | 낱말의 글자 | 이름의 `-` |
| 술어 키 대소문자 | 무시(`Size>1G` = `size>1G`) | 사용자 예시 |

## 4. 코드 지도

| 자리 | 무엇 |
|---|---|
| `crates/nsql-core/src/filterq.rs` | `parse(text) -> Query` · `Query::matches(&dyn Facts)` · `Facts` 포트(`text` · `kind` · `size` · `field`) · `TextFacts` · `parse_size` · `is_structured` · 시험 6(형태 전부 · 논리 · 별칭 · 한글 · 깨진 입력) |
| `crates/nexa-sql/src/filterbar.rs` | `Matcher.query`(구조가 있을 때만 · 정규식 모드 제외) · `Matcher::matches_facts` · `matches`/`matches_cached`도 질의를 탄다 · `NodeFacts`(패널이 채우는 사실) · `FilterBar::matches_facts` · `query_has(key)` |
| `crates/nexa-sql/src/explorer.rs` | `refilter` = 구조 질의면 `NodeFacts { 라벨, kind_name, size_bytes }` · 인덱스 검색 `search_names` = 종류 사실 · `size_bytes`(표시·술어 공용) · `kind_name` · 우클릭 `sizes` = `request_sizes(schema)`(테이블·MV·인덱스 `Req::Sizes`) |
| `crates/nexa-sql/src/project_panel.rs` | `matches` = `NodeFacts { 이름/경로, file\|folder, size(술어가 있을 때만 metadata), ext, path }` |
| i18n | `ExpLoadSizes` "Load sizes"/"용량 확인" |

## 5. 예

| 질의 | 뜻 |
|---|---|
| `size>1G` | 1G 넘는 테이블·MV·인덱스(읽힌 것) |
| `tb (size>1G \| type:view)` | 이름에 `tb` + (1G 초과 또는 뷰) |
| `type:idx size:100M..1G` | 100M~1G 인덱스 |
| `order !hist` | `order` 포함 · `hist` 제외 |
| `ext:sql size>50K`(프로젝트) | 50K 넘는 `.sql` 파일 |

## 6. 후속

- `T-261` 술어 자동 완성(필터 상자에서 `size:` 뒤 단위 · `type:` 뒤 종류 — 팔레트식 드롭다운).
- `T-262` 프로젝트 탐색기 파일 크기 캐시(열거 때 함께 · 지금은 술어가 있을 때 stat).
- `T-263` 북마크 `line:` · 아웃라인 `type:`(루틴/블록) 사실 추가.
