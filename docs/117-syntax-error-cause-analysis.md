# 117 · 구문 오류 원인 분석 — 서버 위치 앞 구간의 키워드 오타 후보 · 첫 후보로 캐럿(T-372 · 132차 win · 10-20)

> **요구 원문(사용자 10-20)**: `SELECT DISTINC a.x FROM t`에 ORA-00923이 "1행"으로만 오는데, 오류 줄의 **어느 위치가 원인인지** 다시 검토해 달라 → 검토 뒤 결정 3 = ① **분리 개발**(별도 부품 · 기존 T-371 길은 그대로) ② 후보가 여럿이면 **첫 번째(가장 앞)로 캐럿** ③ 향상 모드에서는 **강제로 꺼지되 별도 키 없이 `perf::BOOST` 한 곳에서 일괄**.
> **분담** = 설계·구현 = 개발 세션(Fable) · 이 문서 작성 · V1/V2 · journal = 협업 세션 · 결정 = [D-311](10-decision-record.md).
> **짝 문서** = [42](42-db-error-normalization.md) 오류 정규화(`ErrorClass`) · [32](32-server-messages-and-live-log.md) 서버 메시지 · [82](82-grammar-driven-completion.md) 문법 표 `.sqlg`(`next(절)`) · [96](96-object-links.md) `SnapResolver`(메타 스냅숏) · [39 §3-3](39-resource-governance.md) 부하원 원장 · journal [10-19 §1-3 ⑦](journal/2026-10-19.md)(T-371 = 위치 단위 통일) · [10-20 §1](journal/2026-10-20.md).

## 1. 문제 — 파서가 포기한 지점 ≠ 원인

T-371(131차)은 **서버가 알려 준 위치**로 캐럿을 옮긴다. 그 위치는 드라이버마다 단위가 달라도(바이트·글자·줄) 뜻은 하나 = **파서가 더 못 읽고 멈춘 지점**이다. 키워드 오타에서는 이 지점이 원인보다 **뒤**에 놓인다.

| 문장 | 파서가 읽은 모양 | 서버 위치 | 원인 |
|---|---|---|---|
| `SELECT DISTINC a.x FROM t` | `DISTINC` = 식(식별자) · `a` = 그 식의 별칭 · `.`에서 멈춤 | `.` 또는 `a`(Oracle ORA-00923 = 줄만) | `DISTINC` |
| `SELECT DISTINC '가나다' AS x FROM dual` | `DISTINC` = 식 · 리터럴에서 멈춤 | 리터럴(열 16) | `DISTINC`(열 8) |
| `SELECT a FROM t WHRE a = 1` | `t` 뒤 `WHRE` = 별칭 · `a`에서 멈춤 | `a` | `WHRE` |
| `SELECT a FORM t` | `FORM` = 별칭 · `t`에서 멈춤 | `t` | `FORM` |

식별자 오류(ORA-00904 · PG `column … does not exist` 등)는 서버 위치가 이미 정확하다 → 분석 대상이 **아니다**(부류 `ErrorClass::Syntax`만).

## 2. 방식 — 파서가 아니라 "의심 지점 찾기"

원인은 늘 서버 위치와 **같거나 앞**에 있다 → 구간 = **[문장 시작, 서버 위치]**(문장 하나 이하 · 비용의 핵심). 그 구간의 낱말 가운데 다음 셋을 **모두** 만족하는 것이 후보다.

1. **정확한 키워드가 아니다** — 완성 키워드 표(`intel::keywords_for`) + 문법 절 이름(`Grammar::clause_names`) + 문장 시작 낱말(`Grammar::start`) + 보조 표 `EXTRA_EXACT`(구조 낱말 · 넓어도 됨).
2. **메타(캐시)에 있는 이름이 아니다** — 객체 이름(어느 스키마든) · 그 관계에 있는 컬럼 · **한정자로 쓰인 별칭**(`FORM.x`의 `FORM`). 모르면(컬럼 목록 미적재 = `None`) "틀림"이 아니라 **후보로 남긴다**.
3. **그 자리에서 기대되는 키워드와 편집 거리가 가깝다** — 거리 = OSA(Optimal String Alignment · 전위 1회 = 거리 1) · 임계 = 낱말 4~6글자 **≤1** · 7글자 이상 **≤2** · 3글자 이하는 짝이 너무 많아 제외(`SEQ`↔`SET`).

결과는 **위치 순**으로 돌려주고, 호스트는 **첫 번째**로 캐럿을 옮긴다(결정 ②). 서버 왕복 0 · 예산(`deadline`)을 넘으면 그때까지 찾은 것만.

### 2-1. 규칙 표

| 항목 | 규칙 | 코드 |
|---|---|---|
| **구간 시작** | 실패한 문장의 시작(드라이버에 보낸 글 기준 · T-371 `last_run_starts`) | `Input.sql` |
| **구간 끝(위치 있음)** | 서버 위치가 든 **낱말의 끝까지 포함**(그 낱말 자체가 오타일 수 있다 · `SELEC 1`) | `window_end` |
| **구간 끝(줄만 · SQL Server)** | 그 줄에서 `near 'X'` 토큰이 있으면 토큰 끝 · 없으면 그 줄 끝 | `pos_is_line` · `near_token` |
| **구간 끝(위치 없음)** | `near "X"` 토큰(PG · SQLite)이 있으면 그 낱말 끝 · 없으면 문장 끝 | `near_token` |
| **기대 키워드(목표)** | 좁게 = 현재 절의 `.sqlg` `next(절)`(우선 0) → 절 이름 · 문장 시작 · 핵심 표 `CORE_TARGETS`(우선 1) · 4글자 이상만 | `base_targets` · `consider` |
| **절 추적** | 낱말을 훑으며 절 이름(1~3낱말)에서 절을 바꿈 · `(`/`)` = 깊이별 스택(서브쿼리 안은 그 안의 절) | `clause_ending_at` |
| **동률** | 거리 같으면 **공통 접두가 긴 쪽**(`WHER` → WHERE(4) > WHEN(3)) → 같으면 현재 절의 `next` 우선 | `consider` · `common_prefix` |
| **제외 자리 ①** | `.` 앞/뒤 낱말(한정자 · 멤버) | `is_candidate_word` |
| **제외 자리 ②** | 이름이 오는 자리 = 직전 낱말이 `NAME_AFTER`(`AS` · `FROM` · `JOIN` · `INTO` · `UPDATE` · `TABLE` · … · `EXEC`) | `NAME_AFTER` |
| **제외 자리 ③** | 뒤에 `(`가 오는 낱말 = 함수 호출(`PAREN_OK` = `VALUES` · `EXISTS` · `USING` · `OVER` · `PARTITION` · `WITHIN` · `KEEP`만 허용) | `before_paren` |
| **제외 자리 ④** | 인용 낱말 · 영문자/`_`로 시작하지 않는 낱말 | `Word.quoted` |
| **메타 거르기** | `Known::has_object(name)` = 어느 스키마든 객체면 제외 · `has_column(schema, table, col)` = `Some(true)`면 제외 · `None` = 보류(후보 유지) · 별칭은 **한정자로 쓰였을 때만** 보호(`FROM t WHRE a = 1`의 `WHRE`는 별칭 표에 들어오지만 한정자가 아니라 후보) | `Known` 포트 · `used_quals` |
| **상한** | 낱말 20,000 · 64낱말마다 예산 확인 · 예산 지나면 그때까지만 | `MAX_WORDS` · `expired` |
| **출력** | `Suspect{start, end(글자 · 문장 기준), text, expect, dist}` 위치 순 | `analyze` |

### 2-2. 호스트 배선(nexa-sql `app/errsuspect.rs` · `events.rs`)

- **게이트** `analyze_wanted(enabled, class, start)` = 설정 켬 **and** `ErrorClass::Syntax` **and** 문장 시작을 편집기에 맞출 수 있음(`start.is_some()` · 정규화돼 못 맞추면 분석 안 함) — MC/DC 시험.
- **메타 포트** = Ctrl 링크의 `SnapResolver`(같은 스냅숏 · 같은 판정 · `pub(crate) new` + `impl Known`) · 그 세션 칸의 메타만 · 왕복 0.
- **캐럿** = 첫 후보 → 없으면 종전(서버 위치 → 문장 시작 줄). `by_suspect.or(by_pos).or_else(문장 줄)`.
- **Output** 1줄(`Msg::OutErrorSuspects` · Warn) = `Suspected typo: 1:8 'DISTINC' → DISTINCT? (server position 1:16)` · 후보 최대 3 + `+N` · 좌표 = 활성 편집기 1부터 · 서버 위치 없으면 `-`.
- **진단** `NSQL_TRACE_ERRPOS=1` → 로그 `[errpos] suspects=N x.xx ms pos=… [후보]`.

## 3. 설정 · 향상 모드

| 키 | 분류 | 종류 | 기본 | 뜻 |
|---|---|---|---|---|
| `editor.error_analyze` | Editor | Bool | on | 구문 오류 원인 분석(켬/끔) |
| `editor.error_analyze_budget_ms` | Editor · ADVANCED | Int 1~500 | 20 | 분석 한 번의 시간 상한 · 넘으면 서버 위치만 |
| `editor.error_goto`(T-371 · 기존) | Editor | Bool | on | 오류 위치로 캐럿(분석은 이 위에 얹힘) |

- **향상 모드** = `perf::BOOST` 표에 `("editor.error_analyze", "off")` 한 줄(결정 ③ · 별도 키 없음) → 강제 끔 = 서버 위치 그대로 · `editor.error_goto`는 그대로.
- 부하원 원장 = [39 §3-3](39-resource-governance.md)(CPU · 구문 오류당 1회 · 끄는 키 = `editor.error_analyze` · 예산 키).

## 4. 버린 대안

| 대안 | 왜 안 했나 |
|---|---|
| 방언별 **전체 파서**(문법 전부 · 오류 복구) | 4방언 × 문법 전수 유지 비용 · 서버 파서와 어긋나면 더 헷갈린다 · 이 요구(오타 자리)에는 과하다 |
| **서버 재파싱 이분 탐색**(문장을 잘라 다시 보내 원인 자리를 좁힘) | 왕복 N회 · 실서버 부하 · 운영 금지 원칙과 충돌(26 §8 · 39) |
| **외부 crate**(SQL 파서 · 맞춤법) | DR-3(외부 crate 0 지향) · 의존 0 크레이트 `nsql-script` 안에 순수 함수 수백 줄로 충분 |
| 토스트 클릭으로 후보 고르기 | 사용자 결정 ② = 첫 후보로 바로 캐럿(후보 목록은 Output 1줄) |

## 5. 시험

- **단위(nsql-script `errsuspect::tests` 12)** = OSA 전위 거리 1 · `DISTINC` 서버 위치 앞 · 구간 끝 = 서버 위치 낱말 포함 · 정확 키워드·짧은 낱말 제외 · 메타 캐시 이름 제외 · 별칭 한정자·멤버·호출·이름 자리 제외 · 앞 오타 뒤에도 절 키워드 유지 · 서브쿼리 안 오타 · 줄 단위 위치 = 그 줄 구간 · `near` 토큰 파싱 · 예산 만료 = 빈 결과 · 방언 공통 오타 묶음.
- **호스트(nexa-sql `app/errsuspect.rs` 1)** = `analyze_wanted` MC/DC.
- **E2E `scripts/win-error-goto-e2e.sh`**(T-371 ①② 유지 + T-372 ③④⑤) = ③ `SELECT DISTINC '가나다' AS x FROM dual` → 캐럿 1:8(5방언 전부 · SQL Server도 **열 8**) + Output `Suspected typo` · ④ 둘째 문장 = 2:8 · ⑤ SQLite만 `perf.boost=on` → 1:16(강제 끔 = 서버 위치). 기대 = 5방언 × 4 + 1 = 21 케이스. 결과 = journal 10-20 §1.
