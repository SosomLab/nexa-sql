-- Oracle REF CURSOR 예제 패키지(사용자 요청 09-18) — 이름에 글자가 든 객체 목록을 커서로 돌려준다.
-- 실행: 편집기에서 F5(전체) · 또는 `nsql run examples/oracle-refcursor-pkg.sql -c <프로필>`
-- 정리: DROP PACKAGE NSQL_DEMO_PKG;

CREATE OR REPLACE PACKAGE NSQL_DEMO_PKG AS
    -- 약한 타입 커서(열 구성이 고정되지 않음 — 클라이언트가 결과 모양을 그대로 받는다).
    TYPE T_CURSOR IS REF CURSOR;

    -- P_NAME을 이름에 **포함**하는 객체 목록(대소문자 무시 · 내가 볼 수 있는 객체 ALL_OBJECTS).
    --   P_NAME  찾을 글자(NULL·빈 글 = 전부) · `%` `_`는 글자 그대로 찾는다
    --   P_MAX   최대 행 수(기본 200)
    --   P_RC    결과 커서: OWNER · OBJECT_NAME · OBJECT_TYPE · STATUS · CREATED · LAST_DDL_TIME
    PROCEDURE FIND_OBJECTS(
        P_NAME IN  VARCHAR2,
        P_RC   OUT T_CURSOR,
        P_MAX  IN  PLS_INTEGER DEFAULT 200
    );

    -- 같은 질의를 함수로(커서 변수에 대입 한 줄로 받는다).
    FUNCTION OBJECTS_LIKE(P_NAME IN VARCHAR2, P_MAX IN PLS_INTEGER DEFAULT 200) RETURN T_CURSOR;
END NSQL_DEMO_PKG;
/

CREATE OR REPLACE PACKAGE BODY NSQL_DEMO_PKG AS

    PROCEDURE FIND_OBJECTS(
        P_NAME IN  VARCHAR2,
        P_RC   OUT T_CURSOR,
        P_MAX  IN  PLS_INTEGER DEFAULT 200
    ) IS
        -- LIKE 특수문자를 글자로 취급(ESCAPE '\') — 'A_B'를 찾으면 밑줄이 든 이름만.
        V_PAT VARCHAR2(4000) :=
            '%' || REPLACE(REPLACE(REPLACE(UPPER(P_NAME), '\', '\\'), '%', '\%'), '_', '\_') || '%';
    BEGIN
        OPEN P_RC FOR
            SELECT OWNER, OBJECT_NAME, OBJECT_TYPE, STATUS, CREATED, LAST_DDL_TIME
              FROM (SELECT O.OWNER, O.OBJECT_NAME, O.OBJECT_TYPE, O.STATUS, O.CREATED, O.LAST_DDL_TIME
                      FROM ALL_OBJECTS O
                     WHERE P_NAME IS NULL
                        OR UPPER(O.OBJECT_NAME) LIKE V_PAT ESCAPE '\'
                     ORDER BY O.OWNER, O.OBJECT_NAME, O.OBJECT_TYPE)
             WHERE ROWNUM <= NVL(P_MAX, 200);
    END FIND_OBJECTS;

    FUNCTION OBJECTS_LIKE(P_NAME IN VARCHAR2, P_MAX IN PLS_INTEGER DEFAULT 200) RETURN T_CURSOR IS
        V_RC T_CURSOR;
    BEGIN
        FIND_OBJECTS(P_NAME, V_RC, P_MAX);
        RETURN V_RC;
    END OBJECTS_LIKE;

END NSQL_DEMO_PKG;
/

SHOW ERRORS

-- ── 사용 예(SQL*Plus 관용 그대로 · 세션 변수는 클라이언트에 산다 DR-8)
VARIABLE rc REFCURSOR
EXEC :V_NAME := 'EMP'

-- ① 프로시저: OUT 커서를 바인드 변수로 받는다.
EXEC NSQL_DEMO_PKG.FIND_OBJECTS(:V_NAME, :rc)
PRINT rc

-- ② 행 수 제한(이름 지정 인자).
EXEC NSQL_DEMO_PKG.FIND_OBJECTS(P_NAME => 'DUAL', P_RC => :rc, P_MAX => 10)
PRINT rc

-- ③ 함수: 대입 한 줄.
EXEC :rc := NSQL_DEMO_PKG.OBJECTS_LIKE('V$SESS', 20)
PRINT rc
