-- PostgreSQL 실기 스크립트(nsql run -c matrixdb2 examples/it-pg.sql) — 세션 변수 · 프로시저 OUT · 타입 표시.
EXEC :V_NAME := 'nexa'
EXEC SELECT COUNT(*) INTO :V_CNT FROM (SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3) t
PRINT V_CNT
SELECT :V_NAME AS n, :V_CNT AS c, current_database() AS db, version() AS v;
CREATE OR REPLACE PROCEDURE nsql_it_p(IN a INT, OUT b INT) LANGUAGE plpgsql AS $$ BEGIN b := a * 2; END $$;
CALL nsql_it_p(21, NULL);
SELECT 1234.5::numeric AS d, DATE '2026-07-23' AS dt, TIMESTAMP '2026-07-23 01:02:03' AS ts, now() AS tz, gen_random_uuid() AS u, '{"a":1}'::jsonb AS j;
DROP PROCEDURE nsql_it_p(INT, INT);
