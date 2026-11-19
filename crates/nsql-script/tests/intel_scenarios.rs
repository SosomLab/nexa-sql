//! ★ 자동완성 **범위·파생 테이블 컬럼** 시나리오(사용자 11-19 "시나리오가 분명하면 시험 자동화로 충분히 검증") — 문맥 수준.
//!
//! 한 줄 = 한 시나리오: `|` = 캐럿인 SQL · 물을 한정자 · 기대(그 한정자가 무엇으로 풀려야 하는가).
//! 후보 목록(팝업에 실제로 나오는 글) 수준은 `nexa-sql/src/intel.rs`의 `scenario_*` 시험이 같은 문장들로 본다.
#![allow(clippy::unwrap_used)]

use nsql_core::Dialect;
use nsql_script::intel::{context_at, resolve_alias, Context, CtxKind};

/// 한정자가 풀려야 하는 모양.
#[derive(Debug)]
enum Want {
    /// 파생 테이블(인라인 뷰 · CTE) — 이름을 아는 출력 컬럼 전부(순서대로) + `*`로 내보낸 실테이블.
    Derived(&'static [&'static str], &'static [&'static str]),
    /// 실테이블(카탈로그에서 컬럼을 읽는다).
    Table(&'static str),
    /// 이 자리에서는 안 보인다(별칭 표에 없음).
    Hidden,
}

fn ctx_in(sql: &str, dialect: Dialect) -> Context {
    // 캐럿 = 홀로 선 `|`(SQL의 `||` 연결 연산자는 캐럿이 아니다).
    let b = sql.as_bytes();
    let lone: Vec<usize> = (0..b.len())
        .filter(|&i| b[i] == b'|' && (i == 0 || b[i - 1] != b'|') && b.get(i + 1) != Some(&b'|'))
        .collect();
    assert_eq!(lone.len(), 1, "캐럿은 하나: {sql}");
    let caret = lone[0];
    let text = format!("{}{}", &sql[..caret], &sql[caret + 1..]);
    context_at(&text, caret, Some(dialect))
}

fn check_in(name: &str, sql: &str, dialect: Dialect, qualifier: &str, want: &Want) {
    let c = ctx_in(sql, dialect);
    let got = resolve_alias(&c.aliases, qualifier);
    match want {
        Want::Hidden => assert!(
            got.is_none(),
            "[{name}] `{qualifier}`는 안 보여야 한다: {got:?}"
        ),
        Want::Table(t) => {
            let a = got.unwrap_or_else(|| panic!("[{name}] `{qualifier}` 없음: {:?}", c.aliases));
            assert!(
                a.table.eq_ignore_ascii_case(t) && !a.local && a.derived.is_none(),
                "[{name}] `{qualifier}` = 실테이블 {t}여야 한다: {a:?}"
            );
        }
        Want::Derived(cols, stars) => {
            let a = got.unwrap_or_else(|| panic!("[{name}] `{qualifier}` 없음: {:?}", c.aliases));
            let d = a
                .derived
                .as_ref()
                .unwrap_or_else(|| panic!("[{name}] `{qualifier}` = 파생이어야 한다: {a:?}"));
            let got_cols: Vec<&str> = d.cols.iter().map(String::as_str).collect();
            assert_eq!(&got_cols, cols, "[{name}] `{qualifier}`의 컬럼");
            let got_stars: Vec<&str> = d.stars.iter().map(|(_, t)| t.as_str()).collect();
            assert_eq!(&got_stars, stars, "[{name}] `{qualifier}`의 `*` 실테이블");
        }
    }
}

fn check(name: &str, sql: &str, qualifier: &str, want: &Want) {
    check_in(name, sql, Dialect::Oracle, qualifier, want);
}

/// 사용자 QA 문장(11-19)의 **익명화판** — 모양(탭 들여쓰기 · 줄 주석 · CASE 여러 줄 · `1=1` · 세 겹 인라인 뷰 · 블록마다 같은 별칭 `A`)은
/// 그대로 · 테이블·컬럼 이름만 바꿨다. `@1`~`@9` = 캐럿을 넣을 자리.
const QA: &str = "SELECT
	*
FROM
	(
		SELECT
			A.SO_NO
		,	A.SO_SEQ
		,	A.ITEM_CD
		,	A.PROD_YN
		,	CASE
				WHEN NVL(A.ORDER_QTY, 0) - NVL(C.INV_QTY, 0) = 0 THEN 'Y'
				WHEN B.SALES_ORDER IS NULL THEN 'N'
				ELSE 'Y'
			END				AS	EXEC_YN
		,	A.ORDER_QTY
		,	B.IN_ITEM_CD
		,	B.ALT_BOM
		,	CASE WHEN A.ITEM_CD = B.IN_ITEM_CD THEN 'Y' WHEN B.IN_ITEM_CD IS NULL THEN 'Y' ELSE 'N' END	AS	ITEM_CD_CHECK	--	같으면 문제 없음, (없는 경우)는 불일치 아님
		,	A.DLVR_YMD
		,	C.INV_QTY
		,	NVL(A.ORDER_QTY, 0) - NVL(C.INV_QTY, 0)	AS	PROD_QTY
		,	C.LOC_LIST
		,	CASE WHEN A.ITEM_CD LIKE 'P%' THEN 'P' WHEN A.ITEM_CD LIKE 'R%' THEN 'R' ELSE 'N' END	AS	ITEM_TYPE
		@8
		FROM
			(
				SELECT
					A.SO_NO
				,	A.SO_SEQ
				,	A.ITEM_CD
				,	'Y'			AS	PROD_YN
				,	MAX(A.REQ_QTY)			AS	ORDER_QTY
				,	MAX(A.DLVR_YMD)			AS	DLVR_YMD
				FROM
					T_ORDER_REQ A
				WHERE 1=1
				@1
				GROUP BY
					A.SO_NO
				,	A.SO_SEQ
				,	A.ITEM_CD
			) A
		LEFT OUTER JOIN
			(
				SELECT
					A.SALES_ORDER
				,	A.SALES_SEQ
				,	A.IN_ITEM_CD
				,	A.ALT_BOM
				FROM
					T_BOM_RUN A
				WHERE 1=1
				@2
				GROUP BY
					A.SALES_ORDER
				,	A.SALES_SEQ
				,	A.IN_ITEM_CD
				,	A.ALT_BOM
			) B
		ON 1=1
		AND	B.SALES_ORDER	=	A.SO_NO
		AND	B.SALES_SEQ		=	A.SO_SEQ
		@4
		LEFT OUTER JOIN
			(
				SELECT
					A.SO_NO
				,	A.SO_ITEM_NO
				,	SUM(A.INV_QTY)	AS	INV_QTY
				,	LISTAGG(DISTINCT A.LOC_CD, ',') WITHIN GROUP (ORDER BY A.LOC_CD)	AS	LOC_LIST
				FROM
					T_STOCK A
				WHERE 1=1
				AND	A.YMD	=	'20260101'
				AND	A.SO_NO		IS NOT NULL
				@3
				GROUP BY
					A.SO_NO
				,	A.SO_ITEM_NO
			) C
			ON 1=1
			AND	C.SO_NO			=	A.SO_NO
			AND	C.SO_ITEM_NO	=	A.SO_SEQ
			@5
		WHERE 1=1
		@6
	) A
WHERE 1=1
AND	A.EXEC_YN		=	'N'
-- AND	A.ITEM_CD_CHECK		=	'Y'	--	불일치
@7
ORDER BY
	A.SO_NO
,	A.SO_SEQ
@9
;
";

const QA_MARKS: [&str; 9] = ["@1", "@2", "@3", "@4", "@5", "@6", "@7", "@8", "@9"];

/// `mark` 자리에 `put`(캐럿 `|` 포함)을 넣고 나머지 표식은 지운다.
fn qa_at(mark: &str, put: &str) -> String {
    let mut s = QA.to_string();
    for m in QA_MARKS {
        s = s.replace(m, if m == mark { put } else { "" });
    }
    s
}

const A_INNER: &[&str] = &[
    "SO_NO",
    "SO_SEQ",
    "ITEM_CD",
    "PROD_YN",
    "ORDER_QTY",
    "DLVR_YMD",
];
const B_COLS: &[&str] = &["SALES_ORDER", "SALES_SEQ", "IN_ITEM_CD", "ALT_BOM"];
const C_COLS: &[&str] = &["SO_NO", "SO_ITEM_NO", "INV_QTY", "LOC_LIST"];
const A_OUTER: &[&str] = &[
    "SO_NO",
    "SO_SEQ",
    "ITEM_CD",
    "PROD_YN",
    "EXEC_YN",
    "ORDER_QTY",
    "IN_ITEM_CD",
    "ALT_BOM",
    "ITEM_CD_CHECK",
    "DLVR_YMD",
    "INV_QTY",
    "PROD_QTY",
    "LOC_LIST",
    "ITEM_TYPE",
];

/// ★ 사용자 화면 1·2·3: 가운데 블록(JOIN … ON)에서 `C.` = C 서브쿼리의 네 컬럼 · `A.` = 첫 인라인 뷰(LOC_LIST 없음) · `B.` = 넷.
#[test]
fn qa_middle_block_on_clause() {
    for mark in ["@4", "@5", "@6"] {
        let name = format!("가운데 {mark}");
        check(
            &name,
            &qa_at(mark, "AND A.|"),
            "A",
            &Want::Derived(A_INNER, &[]),
        );
        check(
            &name,
            &qa_at(mark, "AND B.|"),
            "B",
            &Want::Derived(B_COLS, &[]),
        );
        check(
            &name,
            &qa_at(mark, "AND C.|"),
            "C",
            &Want::Derived(C_COLS, &[]),
        );
    }
    // 화면 3 그대로: `AND A.STR|` — 접두가 있어도 같은 별칭(LOC_LIST·STR…는 A에 없다).
    let c = ctx_in(&qa_at("@5", "AND	A.STR|"), Dialect::Oracle);
    assert_eq!(
        c.kind,
        CtxKind::Member {
            qualifier: "A".into()
        }
    );
    assert_eq!(c.prefix, "STR");
    let a = resolve_alias(&c.aliases, "A").unwrap();
    let cols = &a.derived.as_ref().unwrap().cols;
    assert!(
        !cols.iter().any(|c| c.starts_with("STR") || c == "LOC_LIST"),
        "{cols:?}"
    );
}

/// 가운데 블록의 SELECT 목록(FROM보다 앞)에서도 같은 블록의 별칭.
#[test]
fn qa_middle_block_select_list() {
    check(
        "목록",
        &qa_at("@8", ",	C.|"),
        "C",
        &Want::Derived(C_COLS, &[]),
    );
    check(
        "목록",
        &qa_at("@8", ",	A.|"),
        "A",
        &Want::Derived(A_INNER, &[]),
    );
    check(
        "목록",
        &qa_at("@8", ",	B.|"),
        "B",
        &Want::Derived(B_COLS, &[]),
    );
}

/// 안쪽 블록 셋: `A` = 그 블록의 실테이블(블록마다 다르다).
#[test]
fn qa_inner_blocks_resolve_their_own_table() {
    for (mark, table) in [
        ("@1", "T_ORDER_REQ"),
        ("@2", "T_BOM_RUN"),
        ("@3", "T_STOCK"),
    ] {
        check(mark, &qa_at(mark, "AND A.|"), "A", &Want::Table(table));
    }
}

/// 맨 바깥(WHERE · ORDER BY): `A` = 바깥 인라인 뷰의 SELECT 목록 14개(CASE … END AS · 식 AS · 줄 주석 포함) · B·C는 안 보인다.
#[test]
fn qa_outer_block() {
    for (mark, put) in [("@7", "AND A.|"), ("@9", ",	A.|")] {
        let sql = qa_at(mark, put);
        check(mark, &sql, "A", &Want::Derived(A_OUTER, &[]));
        check(mark, &sql, "B", &Want::Hidden);
        check(mark, &sql, "C", &Want::Hidden);
    }
}

/// ★ 11-19(사용자 "FROM 절의 `A.` 뒤에는 컬럼이 들어갈 수 없다"): 관계의 별칭 자리 = 후보 없음 · 컬럼 자리(ON·WHERE·CONNECT BY ·
/// 함수 괄호)와 한정 이름(`FROM 스키마.|`)은 그대로.
#[test]
fn relation_alias_position_offers_nothing() {
    use nsql_script::intel::CtxKind;
    let none = [
        "SELECT a.x FROM t1 A.| GROUP BY a.x",
        "SELECT * FROM s1.t1 A.|",
        "SELECT * FROM t1 AS A.|",
        "SELECT * FROM t1 x JOIN t2 A.| ON 1=1",
        "SELECT * FROM t1 x, t2 A.|",
        "SELECT * FROM (SELECT 1 c FROM t1) A.|",
        "SELECT * FROM t1 x LEFT OUTER JOIN (SELECT a.c FROM t2 A.| WHERE 1=1) q ON 1=1",
    ];
    for sql in none {
        let c = ctx_in(sql, Dialect::Oracle);
        assert_eq!(c.kind, CtxKind::None, "{sql}");
    }
    let member = [
        "SELECT * FROM t1 A JOIN t2 B ON A.|",
        "SELECT * FROM t1 A WHERE A.|",
        "SELECT A.| FROM t1 A",
        "SELECT * FROM t1 A CONNECT BY PRIOR A.|",
        "SELECT * FROM t1 A START WITH A.|",
        "SELECT * FROM t1 A FOR UPDATE OF A.|",
        "SELECT * FROM t1 A, TABLE(A.|) B",
        "SELECT * FROM s1.|",
        "SELECT * FROM t1 A, s1.|",
        "UPDATE t1 A SET A.|",
    ];
    for sql in member {
        let c = ctx_in(sql, Dialect::Oracle);
        assert!(
            matches!(c.kind, CtxKind::Member { .. }),
            "{sql}: {:?}",
            c.kind
        );
    }
}

/// 범위 규칙 — 같은 별칭 · 형제 · 상관 서브쿼리 · 함수 괄호.
#[test]
fn scope_rules() {
    let cases: &[(&str, &str, &str, Want)] = &[
        (
            "형제 서브쿼리의 같은 별칭 = 캐럿 블록 것",
            "SELECT * FROM (SELECT x.a FROM t1 x) p JOIN (SELECT x.b FROM t2 x WHERE x.|) q ON 1=1",
            "x",
            Want::Table("t2"),
        ),
        (
            "형제 서브쿼리의 별칭은 바깥에서 안 보인다",
            "SELECT * FROM t1 a WHERE a.id IN (SELECT b.id FROM t2 b) AND b.|",
            "b",
            Want::Hidden,
        ),
        (
            "형제 서브쿼리끼리도 안 보인다",
            "SELECT * FROM (SELECT x.a FROM t1 x) p JOIN (SELECT y.b FROM t2 y WHERE x.|) q ON 1=1",
            "x",
            Want::Hidden,
        ),
        (
            "상관 서브쿼리 = 바깥 별칭이 보인다(SELECT 목록 스칼라)",
            "SELECT (SELECT MAX(x.v) FROM t2 x WHERE x.id = o.|) FROM t1 o",
            "o",
            Want::Table("t1"),
        ),
        (
            "상관 서브쿼리(WHERE EXISTS) — 안쪽 별칭",
            "SELECT * FROM t1 o WHERE EXISTS (SELECT 1 FROM t2 i WHERE i.| = o.id)",
            "i",
            Want::Table("t2"),
        ),
        (
            "상관 서브쿼리(WHERE EXISTS) — 바깥 별칭",
            "SELECT * FROM t1 o WHERE EXISTS (SELECT 1 FROM t2 i WHERE i.id = o.|)",
            "o",
            Want::Table("t1"),
        ),
        (
            "안쪽이 바깥을 가린다",
            "SELECT * FROM t1 a WHERE a.id IN (SELECT a.| FROM t2 a)",
            "a",
            Want::Table("t2"),
        ),
        (
            "서브쿼리를 지나온 뒤에는 다시 바깥 것",
            "SELECT * FROM t1 a WHERE a.id IN (SELECT a.id FROM t2 a) AND a.|",
            "a",
            Want::Table("t1"),
        ),
        (
            "함수 괄호는 범위가 아니다",
            "SELECT NVL(a.|, 0) FROM t1 a",
            "a",
            Want::Table("t1"),
        ),
        (
            "중첩 함수 괄호",
            "SELECT COALESCE(NULLIF(TRIM(a.|), ''), 'x') FROM hr.t1 a",
            "a",
            Want::Table("t1"),
        ),
        (
            "세 겹 — 가장 안쪽",
            "SELECT * FROM (SELECT * FROM (SELECT z.| FROM t3 z) y) x",
            "z",
            Want::Table("t3"),
        ),
        (
            "UPDATE의 상관 서브쿼리",
            "UPDATE t1 a SET a.x = (SELECT MAX(b.x) FROM t2 b WHERE b.id = a.|)",
            "a",
            Want::Table("t1"),
        ),
        (
            "쉼표 조인(Oracle 옛 문법) + 인라인 뷰",
            "SELECT * FROM t1 a, (SELECT k.id, k.nm FROM t2 k) b WHERE a.id = b.|",
            "b",
            Want::Derived(&["id", "nm"], &[]),
        ),
        (
            "입력 중(안 닫힌 서브쿼리) — 안쪽 별칭",
            "SELECT * FROM t0 o JOIN (SELECT a.x FROM t1 a WHERE a.|",
            "a",
            Want::Table("t1"),
        ),
        (
            "입력 중(뒤가 비어 있음) — 인라인 뷰",
            "SELECT * FROM (SELECT a.x, a.y AS yy FROM t1 a) s WHERE s.|",
            "s",
            Want::Derived(&["x", "yy"], &[]),
        ),
    ];
    for (name, sql, q, want) in cases {
        check(name, sql, q, want);
    }
}

/// SELECT 목록 → 출력 컬럼 이름 규칙.
#[test]
fn projection_rules() {
    let cases: &[(&str, &str, &str, Want)] = &[
        (
            "AS 별칭 · 암시 별칭 · 점 사슬 끝 이름",
            "SELECT s.| FROM (SELECT a.id, a.nm AS name, a.amt total, hr.t1.code FROM hr.t1 a) s",
            "s",
            Want::Derived(&["id", "name", "total", "code"], &[]),
        ),
        (
            "이름 없는 식은 빠진다(COUNT(*) · 산술 · 연결 · 리터럴 · CASE)",
            "SELECT s.| FROM (SELECT COUNT(*), a.x + 1, a.x || '-', 'lit', 42, CASE WHEN 1=1 THEN a.x END, a.keep FROM t1 a) s",
            "s",
            Want::Derived(&["keep"], &[]),
        ),
        (
            "집계·분석 함수 + 별칭",
            "SELECT s.| FROM (SELECT MAX(a.x) AS mx, SUM(a.y) sm, ROW_NUMBER() OVER (PARTITION BY a.g ORDER BY a.d DESC) AS rn, LISTAGG(a.c, ',') WITHIN GROUP (ORDER BY a.c) lst FROM t1 a GROUP BY a.g) s",
            "s",
            Want::Derived(&["mx", "sm", "rn", "lst"], &[]),
        ),
        (
            "스칼라 서브쿼리 항목",
            "SELECT s.| FROM (SELECT (SELECT MAX(b.v) FROM t2 b WHERE b.id = a.id) AS mv, (SELECT 1 FROM dual), a.id FROM t1 a) s",
            "s",
            Want::Derived(&["mv", "id"], &[]),
        ),
        (
            "문자열 속 쉼표·괄호·FROM · 주석 속 괄호",
            "SELECT s.| FROM (SELECT 'a,b) FROM x' AS txt, /* ( , FROM */ a.id -- ) ,\n , a.nm FROM t1 a) s",
            "s",
            Want::Derived(&["txt", "id", "nm"], &[]),
        ),
        (
            "함수 인자 속 FROM(EXTRACT · TRIM)은 목록 끝이 아니다",
            "SELECT s.| FROM (SELECT EXTRACT(YEAR FROM a.d) AS yy, TRIM(BOTH ' ' FROM a.nm) AS nm2 FROM t1 a) s",
            "s",
            Want::Derived(&["yy", "nm2"], &[]),
        ),
        (
            "DISTINCT",
            "SELECT s.| FROM (SELECT DISTINCT a.id, a.nm FROM t1 a) s",
            "s",
            Want::Derived(&["id", "nm"], &[]),
        ),
        (
            "인용 별칭",
            "SELECT s.| FROM (SELECT a.id AS \"Order No\", a.nm \"Name\" FROM t1 a) s",
            "s",
            Want::Derived(&["Order No", "Name"], &[]),
        ),
        (
            "같은 이름은 한 번",
            "SELECT s.| FROM (SELECT a.id, b.id, b.nm FROM t1 a JOIN t2 b ON 1=1) s",
            "s",
            Want::Derived(&["id", "nm"], &[]),
        ),
        (
            "집합 연산 = 첫 SELECT의 이름",
            "SELECT u.| FROM (SELECT a.x, a.y AS yy FROM t1 a UNION ALL SELECT b.p, b.q FROM t2 b) u",
            "u",
            Want::Derived(&["x", "yy"], &[]),
        ),
        (
            "통째로 괄호 친 집합 연산",
            "SELECT u.| FROM ((SELECT a.x FROM t1 a) UNION (SELECT b.p FROM t2 b)) u",
            "u",
            Want::Derived(&["x"], &[]),
        ),
        (
            "`*` = 그 층의 실테이블 전부",
            "SELECT s.| FROM (SELECT * FROM t1 a JOIN hr.t2 b ON 1=1) s",
            "s",
            Want::Derived(&[], &["t1", "t2"]),
        ),
        (
            "`t.*` + 이름 있는 컬럼",
            "SELECT s.| FROM (SELECT b.*, a.id AS aid FROM t1 a JOIN t2 b ON 1=1) s",
            "s",
            Want::Derived(&["aid"], &["t2"]),
        ),
        (
            "`*`가 안쪽 인라인 뷰를 그대로 내보낸다(세 겹)",
            "SELECT o.| FROM (SELECT * FROM (SELECT * FROM (SELECT t.id AS k, t.nm FROM t1 t) i) m) o",
            "o",
            Want::Derived(&["k", "nm"], &[]),
        ),
        (
            "`i.*` = 안쪽 인라인 뷰 + 실테이블 `*`",
            "SELECT o.| FROM (SELECT i.*, r.* FROM (SELECT t.id FROM t1 t) i, t9 r) o",
            "o",
            Want::Derived(&["id"], &["t9"]),
        ),
        (
            "곱셈의 `*`는 별이 아니다",
            "SELECT s.| FROM (SELECT a.x * a.y AS xy, a.z FROM t1 a) s",
            "s",
            Want::Derived(&["xy", "z"], &[]),
        ),
        (
            "AS를 쓴 인라인 뷰 별칭",
            "SELECT s.| FROM (SELECT a.id FROM t1 a) AS s",
            "s",
            Want::Derived(&["id"], &[]),
        ),
    ];
    for (name, sql, q, want) in cases {
        check(name, sql, q, want);
    }
    // 방언 꼬리: SQL Server `TOP (n)` · PostgreSQL `::` 형 변환.
    check_in(
        "TOP (n)",
        "SELECT s.| FROM (SELECT TOP (10) a.id, a.nm AS name FROM t1 a) s",
        Dialect::Mssql,
        "s",
        &Want::Derived(&["id", "name"], &[]),
    );
    check_in(
        "TOP n",
        "SELECT s.| FROM (SELECT TOP 10 a.id FROM t1 a) s",
        Dialect::Mssql,
        "s",
        &Want::Derived(&["id"], &[]),
    );
    check_in(
        "PG 형 변환 + AS",
        "SELECT s.| FROM (SELECT a.id::text AS id_txt, a.nm FROM t1 a) s",
        Dialect::Postgres,
        "s",
        &Want::Derived(&["id_txt", "nm"], &[]),
    );
}

/// CTE — 컬럼 목록 · SELECT 목록 · 사슬 · 별칭 · 서브쿼리 안에서의 참조.
#[test]
fn cte_rules() {
    const CHAIN: &str = "WITH a AS (SELECT t.id, t.nm FROM t1 t @1), \
        b (k, cnt) AS (SELECT a.id, COUNT(*) FROM a @2 GROUP BY a.id), \
        c AS (SELECT * FROM b) \
        SELECT * FROM a x JOIN b ON b.k = x.id JOIN c ON 1=1 WHERE 1=1 @3 \
        AND EXISTS (SELECT 1 FROM (SELECT * FROM a) z WHERE 1=1 @4)";
    let at = |mark: &str, put: &str| {
        let mut s = CHAIN.to_string();
        for m in ["@1", "@2", "@3", "@4"] {
            s = s.replace(m, if m == mark { put } else { "" });
        }
        s
    };
    // 본문 안: 실테이블 별칭 · 앞 CTE.
    check(
        "CTE a 본문",
        &at("@1", "WHERE t.|"),
        "t",
        &Want::Table("t1"),
    );
    check(
        "CTE b 본문에서 앞 CTE a",
        &at("@2", "WHERE a.|"),
        "a",
        &Want::Derived(&["id", "nm"], &[]),
    );
    // 주 쿼리: CTE 별칭 · 컬럼 목록 · `SELECT * FROM b`.
    let main = at("@3", "AND x.|");
    check("FROM a x", &main, "x", &Want::Derived(&["id", "nm"], &[]));
    check("컬럼 목록", &main, "b", &Want::Derived(&["k", "cnt"], &[]));
    check("* FROM cte", &main, "c", &Want::Derived(&["k", "cnt"], &[]));
    // 서브쿼리 안의 인라인 뷰가 바깥 CTE를 `*`로.
    check(
        "서브쿼리 안 (SELECT * FROM a) z",
        &at("@4", "AND z.|"),
        "z",
        &Want::Derived(&["id", "nm"], &[]),
    );
    check(
        "서브쿼리 안에서 주 쿼리의 CTE 별칭",
        &at("@4", "AND x.|"),
        "x",
        &Want::Derived(&["id", "nm"], &[]),
    );
}

/// 어느 자리에 캐럿을 넣어도(QA 문장의 **모든 바이트 경계**) 패닉하지 않고, 별칭 표에 같은 이름이 둘 없다.
#[test]
fn every_caret_position_is_safe_and_unambiguous() {
    let text: String = {
        let mut s = QA.to_string();
        for m in QA_MARKS {
            s = s.replace(m, "");
        }
        s
    };
    for (i, _) in text.char_indices() {
        let c = context_at(&text, i, Some(Dialect::Oracle));
        for (k, a) in c.aliases.iter().enumerate() {
            assert!(
                !c.aliases[..k]
                    .iter()
                    .any(|o| o.alias.eq_ignore_ascii_case(&a.alias)),
                "바이트 {i}: 별칭 `{}` 중복 {:?}",
                a.alias,
                c.aliases
            );
        }
    }
}

/// ★ 11-19 2차: 인라인 뷰 컬럼의 **원본 실테이블 컬럼**(그대로 내보낸 것만 · 블록을 거쳐도 따라간다 · 집계·식은 없음) —
/// Ctrl 객체 설명·링크가 쓴다 · `outer` = 캐럿 블록에서 몇 층 바깥의 별칭인가(`*` 펼치기는 0층만).
#[test]
fn derived_origin_and_outer_rank() {
    let origin = |c: &Context, alias: &str, col: &str| -> Option<(String, String)> {
        resolve_alias(&c.aliases, alias)?
            .derived
            .as_ref()?
            .origin(col)
            .map(|(_, t, n)| (t.clone(), n.clone()))
    };
    let own = |t: &str, n: &str| Some((t.to_string(), n.to_string()));
    // 가운데 블록.
    let c = ctx_in(&qa_at("@5", "AND A.|"), Dialect::Oracle);
    assert_eq!(origin(&c, "A", "SO_NO"), own("T_ORDER_REQ", "SO_NO"));
    assert_eq!(origin(&c, "A", "ORDER_QTY"), None, "MAX(…) = 원본 없음");
    assert_eq!(origin(&c, "A", "PROD_YN"), None, "리터럴 = 원본 없음");
    assert_eq!(origin(&c, "B", "ALT_BOM"), own("T_BOM_RUN", "ALT_BOM"));
    assert_eq!(origin(&c, "C", "SO_ITEM_NO"), own("T_STOCK", "SO_ITEM_NO"));
    assert_eq!(
        origin(&c, "C", "INV_QTY"),
        None,
        "SUM(…) AS 같은 이름 = 원본 없음"
    );
    assert!(
        c.aliases.iter().take(3).all(|a| a.outer == 0),
        "{:?}",
        c.aliases
    );
    // 맨 바깥: 두 블록을 거쳐 원본까지.
    let c = ctx_in(&qa_at("@7", "AND A.|"), Dialect::Oracle);
    assert_eq!(origin(&c, "A", "SO_SEQ"), own("T_ORDER_REQ", "SO_SEQ"));
    assert_eq!(
        origin(&c, "A", "IN_ITEM_CD"),
        own("T_BOM_RUN", "IN_ITEM_CD")
    );
    assert_eq!(origin(&c, "A", "LOC_LIST"), None, "LISTAGG 결과");
    assert_eq!(origin(&c, "A", "EXEC_YN"), None, "CASE 식");
    // 안쪽 블록: 그 블록의 A = 0층 · 바깥 블록의 C = 1층.
    let c = ctx_in(&qa_at("@3", "AND A.|"), Dialect::Oracle);
    let a = resolve_alias(&c.aliases, "A").unwrap();
    assert_eq!((a.table.as_str(), a.outer), ("T_STOCK", 0));
    assert_eq!(resolve_alias(&c.aliases, "C").unwrap().outer, 1);
    // 함수 괄호 안의 `*`도 그 SELECT의 FROM이 0층.
    let c = ctx_in("SELECT COUNT(*|) FROM t1 a WHERE 1=1", Dialect::Oracle);
    assert_eq!(resolve_alias(&c.aliases, "a").unwrap().outer, 0);
}

/// ★ 11-19(사용자 "UNION을 쓰면서 같은 별칭을 쓰는 경우"): 집합 연산으로 이어진 SELECT들은 같은 괄호 층이어도 별칭이 서로 보이지
/// 않는다 — 캐럿이 든 가지의 FROM만(종전 = 처음 것이 이겨 둘째 가지의 `A.`가 첫 가지 테이블로 풀렸다).
#[test]
fn set_operator_branches_resolve_their_own_alias() {
    let o = Dialect::Oracle;
    for (name, sql, table) in [
        (
            "union-1",
            "SELECT A.| FROM t1 A UNION ALL SELECT A.c2 FROM t2 A",
            "t1",
        ),
        (
            "union-2",
            "SELECT A.c1 FROM t1 A UNION ALL SELECT A.| FROM t2 A",
            "t2",
        ),
        (
            "union-2-where",
            "SELECT A.c1 FROM t1 A UNION SELECT A.c2 FROM t2 A WHERE A.|",
            "t2",
        ),
        (
            "minus-3",
            "SELECT A.c1 FROM t1 A MINUS SELECT A.c2 FROM t2 A INTERSECT SELECT A.| FROM t3 A",
            "t3",
        ),
        (
            "except",
            "SELECT A.c1 FROM t1 A EXCEPT SELECT A.| FROM t2 A",
            "t2",
        ),
        // 인라인 뷰 안의 UNION: 가지는 그 괄호 층에서 가른다.
        (
            "inline-union",
            "SELECT X.c FROM (SELECT A.c FROM t1 A UNION ALL SELECT A.| FROM t2 A) X",
            "t2",
        ),
        // 가지 안의 서브쿼리: 안쪽 층이 이기고 바깥은 그 가지의 것.
        (
            "branch-subquery-outer",
            "SELECT 1 FROM t1 A UNION ALL SELECT 1 FROM t2 A WHERE EXISTS (SELECT 1 FROM t9 B WHERE B.k = A.|)",
            "t2",
        ),
    ] {
        check_in(name, sql, o, "A", &Want::Table(table));
    }
    // 다른 가지에만 있는 별칭은 안 보인다.
    check_in(
        "other-branch-hidden",
        "SELECT B.c FROM t1 B UNION ALL SELECT A.| FROM t2 A",
        o,
        "B",
        &Want::Hidden,
    );
    // 첫 가지 앞의 WITH는 모든 가지에서 보인다.
    check_in(
        "cte-visible-in-later-branch",
        "WITH c AS (SELECT k, v FROM t0) SELECT 1 FROM t1 A UNION ALL SELECT c.| FROM c",
        o,
        "c",
        &Want::Derived(&["k", "v"], &[]),
    );
    // 바깥 층의 UNION은 안쪽 블록에서 볼 때도 그 블록이 든 가지만(상관 서브쿼리의 바깥 = 그 가지).
    check_in(
        "outer-level-branch",
        "SELECT 1 FROM t1 A UNION ALL SELECT (SELECT MAX(B.v) FROM t9 B WHERE B.k = A.|) FROM t2 A",
        o,
        "A",
        &Want::Table("t2"),
    );
}

/// ★ 11-19(사용자 "프로시저 전체가 한 문장으로 잡힌다"): PL/SQL 블록(단독 `/`까지 한 단위) 안의 문장들은 `;`로 갈라 본다 —
/// 형제 문장의 같은 별칭이 섞이지 않고, 문장 구간도 캐럿이 든 `;` 문장 하나.
#[test]
fn plsql_block_statements_are_scoped_by_semicolon() {
    let o = Dialect::Oracle;
    let body = |put: &str| {
        format!(
            "CREATE OR REPLACE PROCEDURE p IS\n  v NUMBER;\nBEGIN\n  UPDATE t1 T SET T.c = 1 WHERE T.k = 1;\n  IF SQL%ROWCOUNT = 0 THEN\n    INSERT INTO t3 VALUES (1);\n  END IF;\n  UPDATE t2 T SET T.d = 2 WHERE {put};\n  DELETE FROM t4 X WHERE X.k = 'a;b';\nEND;\n"
        )
    };
    // 둘째 UPDATE의 `T` = t2(첫 UPDATE의 t1이 아니다).
    check_in("sibling-dml", &body("T.|"), o, "T", &Want::Table("t2"));
    // 다른 문장의 별칭은 안 보인다.
    check_in("other-stmt-hidden", &body("T.|"), o, "X", &Want::Hidden);

    // 문장 구간: 캐럿이 든 `;` 문장 하나 · 문자열 안의 `;`는 세지 않는다.
    let text = body("T.k = 2");
    let at = |needle: &str| text.find(needle).unwrap();
    let span = |pos: usize| {
        let (s, e) = nsql_script::intel::inner_statement_span(&text, pos);
        text[s..e].trim().to_string()
    };
    assert_eq!(
        span(at("T.d = 2")),
        "UPDATE t2 T SET T.d = 2 WHERE T.k = 2;"
    );
    assert_eq!(
        span(at("X.k")),
        "DELETE FROM t4 X WHERE X.k = 'a;b';",
        "문자열 안의 `;`"
    );
    assert_eq!(
        span(at("INSERT INTO t3")),
        "IF SQL%ROWCOUNT = 0 THEN\n    INSERT INTO t3 VALUES (1);"
    );
    // `;` 바로 뒤 = 그 `;`로 끝난 문장.
    let after = at("WHERE T.k = 1;") + "WHERE T.k = 1;".len();
    // (앞의 `BEGIN`·`IF … THEN` 같은 제어 낱말은 그 문장 구간에 딸린다 — 경계는 `;`뿐.)
    assert_eq!(
        span(after),
        "BEGIN\n  UPDATE t1 T SET T.c = 1 WHERE T.k = 1;"
    );
    // 안쪽 `;`가 없는 보통 문장 = 단위 전체.
    let one = "SELECT a FROM t WHERE b = 1";
    assert_eq!(
        nsql_script::intel::inner_statement_span(one, 5),
        (0, one.len())
    );
}

/// ★ 11-19(사용자 "별칭 자리에 컬럼 ALT_BOM이 나온다"): 관계 이름 바로 뒤 = 별칭을 적는 자리 → `AliasSlot`(호스트 = 키워드만).
/// Oracle DB 링크 꼬리(`t@link`)가 붙어도 같다 · 별칭 뒤·ON·WHERE는 식 자리 그대로.
#[test]
fn alias_slot_after_relation_name() {
    let o = Dialect::Oracle;
    for sql in [
        "SELECT * FROM t1 A|",
        "SELECT * FROM t1 |",
        "SELECT * FROM s1.t1 A|",
        "SELECT * FROM t1@lnk A|",
        "SELECT * FROM s1.t1@lnk.example.com A|",
        "SELECT * FROM t1 AS A|",
        "SELECT * FROM t1 x JOIN t2 W|",
        "SELECT * FROM t1 x, t2 y|",
        "UPDATE t1 S|",
    ] {
        assert_eq!(ctx_in(sql, o).kind, CtxKind::AliasSlot, "{sql}");
    }
    for sql in [
        "SELECT * FROM t1 A W|",
        "SELECT * FROM t1 A WHERE A|",
        "SELECT * FROM t1 x JOIN t2 y ON y|",
        "SELECT A| FROM t1 A",
    ] {
        assert_eq!(ctx_in(sql, o).kind, CtxKind::Expr, "{sql}");
    }
    // 관계 이름 자체를 치는 중 = 관계 자리 그대로.
    assert_eq!(ctx_in("SELECT * FROM t|", o).kind, CtxKind::Relation);
    // DB 링크가 붙은 테이블의 별칭도 풀린다 · 링크 뒤 점(`A.`)은 별칭 자리 = 후보 없음.
    check_in(
        "dblink-alias",
        "SELECT A.| FROM t1@lnk A",
        o,
        "A",
        &Want::Table("t1"),
    );
    assert_eq!(ctx_in("SELECT * FROM t1@lnk A.|", o).kind, CtxKind::None);
}

/// ★ 11-19(사용자 "`테이블@`를 치면 링크 목록"): Oracle에서 관계 이름에 바로 붙은 `@` 뒤 = DB 링크 자리 — 접두·치환 구간에 `@`는 없다.
#[test]
fn oracle_dblink_position() {
    use nsql_script::intel::Want as W;
    let o = Dialect::Oracle;
    for (sql, prefix) in [
        ("SELECT * FROM t1@|", ""),
        ("SELECT * FROM s1.t1@|", ""),
        ("SELECT * FROM t1@SB|", "SB"),
        ("SELECT * FROM t1 x JOIN s1.t2@LN| y ON 1=1", "LN"),
        ("UPDATE t1@| SET c = 1", ""),
        ("SELECT * FROM t0 a, t1@P|", "P"),
    ] {
        let c = ctx_in(sql, o);
        assert_eq!(c.kind, CtxKind::Want(W::DbLink), "{sql}");
        assert_eq!(c.prefix, prefix, "{sql}");
        // 치환 구간 = 링크 이름만(`@` 뒤부터 캐럿까지).
        assert_eq!(c.replace.len(), prefix.len(), "{sql}");
    }
    // 해당 없음: 공백을 둔 `@` · 별칭 뒤 · 식 자리 · 다른 방언(SQL Server 변수).
    for (sql, d) in [
        ("SELECT * FROM t1 @|", o),
        ("SELECT * FROM t1 A@|", o),
        ("SELECT c@| FROM t1", o),
        ("SELECT * FROM t1 WHERE c = x@|", o),
        ("SELECT * FROM t1@|", Dialect::Mssql),
    ] {
        assert_ne!(ctx_in(sql, d).kind, CtxKind::Want(W::DbLink), "{sql}");
    }
}

/// ★ 11-19(사용자 "링크가 붙은 테이블에도 Alias 붙이기"): DB 링크 자리의 자동 별칭 허용 = 테이블 자리와 같은 규칙(관계 이름 앞 낱말) ·
/// 뒤에 별칭이 이미 있으면 붙이지 않는다(`alias_follows`).
#[test]
fn dblink_position_alias_rules() {
    use nsql_script::intel::Want as W;
    let o = Dialect::Oracle;
    for (sql, ok, follows) in [
        ("SELECT * FROM t1@|\nWHERE 1=1", true, false),
        ("SELECT * FROM s1.t1@LN|", true, false),
        ("SELECT * FROM t0 a JOIN t1@|", true, false),
        ("SELECT * FROM t0 a, t1@|", true, false),
        ("UPDATE t1@| SET c = 1", true, false),
        // 링크 이름 끝에서 다시 고르기 — 뒤가 키워드(INNER JOIN · WHERE)면 별칭이 뒤따르는 것이 아니다(사용자 11-19).
        (
            "SELECT *\nFROM\n\tt1@LN|\nINNER JOIN t2 B\n\tON 1=1",
            true,
            false,
        ),
        ("SELECT * FROM t1@LN| WHERE 1=1", true, false),
        // 이미 별칭이 뒤따른다 = 이름만 바꾼다.
        ("SELECT * FROM t1@| A WHERE 1=1", true, true),
        // 별칭을 허용하지 않는 자리.
        ("INSERT INTO t1@|", false, false),
        ("TRUNCATE TABLE t1@|", false, false),
    ] {
        let c = ctx_in(sql, o);
        assert_eq!(c.kind, CtxKind::Want(W::DbLink), "{sql}");
        assert_eq!((c.alias_ok, c.alias_follows), (ok, follows), "{sql}");
    }
}

/// ★ 11-19(사용자 "union 및 유사 명령은 문장이 분리되므로 A부터 다시 시작"): 자동 별칭이 보는 "이미 쓰인 별칭" = 캐럿이 든 가지의
/// 것뿐 — 앞 가지의 `A`는 보이지 않는다(가지마다 A부터). 테이블 이름을 치는 중(관계 자리)의 문맥으로 본다.
#[test]
fn auto_alias_restarts_in_each_set_operator_branch() {
    let o = Dialect::Oracle;
    let visible = |sql: &str| -> Vec<String> {
        let c = ctx_in(sql, o);
        assert_eq!(c.kind, CtxKind::Relation, "{sql}");
        assert!(c.alias_ok, "{sql}");
        c.aliases
            .iter()
            .filter(|a| !a.alias.eq_ignore_ascii_case(&c.prefix))
            .map(|a| a.alias.clone())
            .collect()
    };
    let head = "select a.ITEM_CD from t1 A\nunion all\nselect a.ITEM_CD from t2 A\nunion all\nselect * from t3 A\n";
    for tail in [
        "union all\nselect * from t4|",
        "union all\nselect * from |",
        "union\nselect * from T|",
        "minus\nselect * from T|",
        "intersect\nselect * from T|",
    ] {
        let sql = format!("{head}{tail}");
        assert!(visible(&sql).is_empty(), "{sql}: {:?}", visible(&sql));
    }
    // 가운데 가지를 고쳐 쓸 때도 그 가지만(뒤 가지의 별칭도 안 보인다).
    let mid = "select 1 from t1 A\nunion all\nselect * from T|\nunion all\nselect * from t3 B";
    assert!(visible(mid).is_empty(), "{:?}", visible(mid));
    // 같은 가지 안의 JOIN은 종전대로 다음 글자.
    let join = "select 1 from t1 A\nunion all\nselect * from t2 A join T|";
    assert_eq!(visible(join), vec!["A".to_string()]);
    // 집합 연산자를 아직 안 썼으면 같은 문장 = 앞의 별칭이 보인다(그래서 다음 글자가 된다).
    let same = "select * from t3 A\nselect * from T|";
    assert_eq!(visible(same), vec!["A".to_string()]);
}
