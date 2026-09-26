-- SQL Server 실기 스크립트: 같은 관용이 T-SQL로 재작성되어 동작한다
EXEC :V_PRG_NM := 'SP_M4P_MPO_M4E_CREATE_BSY'
EXEC
SELECT COUNT(*), MAX(name)
INTO :V_CNT, :V_MAX
FROM sys.objects
WHERE object_id < 100

PRINT V_PRG_NM V_CNT V_MAX
SELECT :V_PRG_NM AS prg, :V_CNT AS cnt, :V_MAX AS mx;
:setvar Top 3
SELECT TOP &Top name FROM sys.objects ORDER BY name
GO

SELECT :Top from dual;

SHOW VARIABLES