-- MySQL/MariaDB 실기 스크립트(integration.yml · Codespaces): 같은 관용이 MySQL 문법으로 재작성되어 동작한다
-- 임시 객체만 쓴다(information_schema 읽기 · 사용자 변수) — 실서버에 남는 것 없음
EXEC :V_PRG_NM := 'SP_M4P_MPO_M4E_CREATE_BSY'
EXEC
SELECT COUNT(*), MAX(TABLE_NAME)
INTO :V_CNT, :V_MAX
FROM information_schema.TABLES
WHERE TABLE_SCHEMA = 'information_schema'

PRINT V_PRG_NM V_CNT V_MAX
SELECT :V_PRG_NM AS prg, :V_CNT AS cnt, :V_MAX AS mx;
SELECT TABLE_NAME FROM information_schema.TABLES WHERE TABLE_SCHEMA = 'information_schema' ORDER BY TABLE_NAME LIMIT 3;

SELECT DATABASE() AS db, VERSION() AS ver;  -- MySQL은 FROM 없는 SELECT 허용(dual 불필요)

SHOW VARIABLES
