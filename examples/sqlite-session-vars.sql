-- SQLite로 세션 변수 왕복을 실기 검증하는 스크립트(드라이버 불요 · nsql run -c sqlite::memory:)
CREATE TABLE emp (id INTEGER PRIMARY KEY, name TEXT, dept TEXT, sal INTEGER);
INSERT INTO emp VALUES (1, '홍길동', 'SALES', 300), (2, '김철수', 'DEV', 450), (3, '이영희', 'DEV', 520);

EXEC :V_DEPT := 'DEV'
EXEC :V_MAX := (SELECT MAX(sal) FROM emp WHERE dept = :V_DEPT)
PRINT V_DEPT V_MAX

SELECT name, sal FROM emp WHERE dept = :V_DEPT AND sal < :V_MAX ORDER BY sal;
SELECT :V_DEPT AS dept, COUNT(*) AS cnt FROM emp WHERE dept = :V_DEPT;
DEFINE bonus = 10
SELECT name, sal + &bonus AS with_bonus FROM emp WHERE id = 1;
VARIABLE
