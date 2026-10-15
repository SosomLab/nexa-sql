-- examples/prod-mode-scenario.sql — 운영기(PRD) 점검 시나리오의 쿼리 모음(docs/114 §11 · scripts/prod-mode-scenario.sh가 돌리는 것과 같음).
-- 손으로 따라 할 때: ① 프로필을 운영기로   nsql conn env <프로필> prd      (되돌리기: nsql conn env <프로필> none)
--                     ② GUI에서 그 프로필로 접속해 아래 단계를 순서대로 실행(F5 = 전체 · Ctrl+Enter = 문장) · 각 단계의 "확인"을 본다.
-- 방언 = Oracle(기본). SQLite로 할 때는 맨 아래 블록을 쓴다. 임시 객체 = NSQLT_HOLD_*(끝에 9단계로 정리).

-- ───── 0. 준비(CLI 또는 비운영 접속으로 한 번) ─────
CREATE TABLE NSQLT_HOLD_T (ID NUMBER PRIMARY KEY, V VARCHAR2(20));
INSERT INTO NSQLT_HOLD_T VALUES (1, 'o1');
INSERT INTO NSQLT_HOLD_T VALUES (2, 'o2');
INSERT INTO NSQLT_HOLD_T VALUES (3, 'o3');
COMMIT;
CREATE OR REPLACE PROCEDURE NSQLT_HOLD_PRC AS BEGIN NULL; END;
/

-- ───── 1. 운영 2단 재실행(DDL) ─────
-- 확인: 첫 실행 = 상태줄 "run again within N s"(빨간 토스트 · 서버로 안 감) → 3 s 안에 다시 실행 = 실행(Output done).
CREATE TABLE NSQLT_HOLD_T2 (A NUMBER);
DROP TABLE NSQLT_HOLD_T2;

-- ───── 2. 객체 백업(변경 DDL 전 현재 DDL → 파일) ─────
-- 확인: Output "Object backup: <스키마>.NSQLT_HOLD_PRC (PROCEDURE) → <설정>/backups/objects/<프로필>_<스키마>.NSQLT_HOLD_PRC_PROCEDURE.<yyyyMMddHHmmssSSS>"
--       상태바 저장 메뉴 ▸ "객체 백업 위치 열기"로 파일 확인(17자리 확장자 · SQL로 열림).
CREATE OR REPLACE PROCEDURE NSQLT_HOLD_PRC AS BEGIN NULL; END;
/

-- ───── 3. 운영 보류 커밋 → 상단 Commit 2단 ─────
-- 확인: 실행 뒤 커밋되지 않음(주황 카드 "Production · commit pending" · 탭 배지 ●1 · 다른 세션에서 SELECT = o1)
--       → 상단 Commit 첫 클릭 = 빨간 무장(카드 "다시 누르면 반영 · N초" 초 감소) → 5 s 안 두 번째 클릭(또는 카드 [Commit]) = 커밋 → SELECT = x1.
UPDATE NSQLT_HOLD_T SET V = 'x1' WHERE ID = 1;
-- 다른 세션(CLI)에서:  SELECT V FROM NSQLT_HOLD_T WHERE ID = 1

-- ───── 4. 보류 → Rollback ─────
-- 확인: 보류 뒤 상단 Rollback(확인 없음) → 대기 0 · SELECT = o2.
UPDATE NSQLT_HOLD_T SET V = 'x2' WHERE ID = 2;

-- ───── 5. 스크립트 안 COMMIT = 직전 멈춤 → 확인 뒤 이어 실행 ─────
-- 확인: 첫 UPDATE 뒤 COMMIT 직전에 멈춤(카드 "COMMIT needs your confirmation") → 카드 [Commit] = COMMIT + 남은 UPDATE 실행 → 다시 보류 → 상단 Commit 2단 → SELECT = b3.
UPDATE NSQLT_HOLD_T SET V = 'a3' WHERE ID = 3;
COMMIT;
UPDATE NSQLT_HOLD_T SET V = 'b3' WHERE ID = 3;

-- ───── 6. 보류 중 다른 탭 ─────
-- 탭 A에서 보류를 만든 뒤(아래 첫 줄) 새 탭에서 둘째·셋째 줄을 실행 → 상태줄 "Production changes are held - commit or roll back first"(서버로 안 감) · 넷째 줄(조회)은 허용.
UPDATE NSQLT_HOLD_T SET V = 'zz' WHERE ID = 1;            -- 탭 A(보류)
UPDATE NSQLT_HOLD_T SET V = 'yy' WHERE ID = 1;            -- 탭 B → 차단
CREATE TABLE NSQLT_HOLD_T2 (A NUMBER);                    -- 탭 C(DDL) → 차단
SELECT COUNT(*) AS N FROM NSQLT_HOLD_T;                   -- 탭 D(조회) → 허용
-- 끝에 상단 Rollback으로 보류 해제.

-- ───── 7. 탐색기 ▸ 삭제…(모달 2단 · 백업 뒤 DROP) ─────
-- 객체 탐색기에서 Procedures ▸ NSQLT_HOLD_PRC 우클릭 ▸ 삭제… → 타임아웃 버튼 2단 확인
-- 확인: 창 "sent" · backups/objects에 백업 +1 · 객체가 실제로 사라짐(첫 호출에 전송 = G6) · 보류는 생략.

-- ───── 8. (옵션) 보류 한도 롤백 ─────
-- 설정 tx.prod_idle_limit_min=1 · tx.idle_countdown_secs=5로 두고 UPDATE를 보류한 채 1분 → 카운트다운 카드 → 자동 롤백(설정과 무관하게 롤백 고정).
UPDATE NSQLT_HOLD_T SET V = 'lim' WHERE ID = 2;

-- ───── 9. 정리 ─────
DROP TABLE NSQLT_HOLD_T;
DROP PROCEDURE NSQLT_HOLD_PRC;
DROP TABLE NSQLT_HOLD_T2;
-- 프로필 되돌리기:  nsql conn env <프로필> none

-- ═════ SQLite 판(서버 없이 · nsql conn add Local "sqlite:<경로>/local.sqlite" -d sqlite · nsql conn env Local prd) ═════
-- 0  CREATE TABLE NSQLT_HOLD_T (ID INTEGER PRIMARY KEY, V TEXT); INSERT … (1,'o1'),(2,'o2'),(3,'o3') · CREATE VIEW NSQLT_HOLD_V AS SELECT ID FROM NSQLT_HOLD_T;
-- 1  CREATE TABLE NSQLT_HOLD_T2 (A INTEGER);
-- 2  DROP VIEW NSQLT_HOLD_V;                      (SQLite = 뷰 DROP이 백업 대상)
-- 3~6  위와 같음(끝에 세미콜론)
-- 7  탐색기 ▸ Views ▸ NSQLT_HOLD_V ▸ 삭제…
-- 9  DROP TABLE NSQLT_HOLD_T; DROP VIEW NSQLT_HOLD_V;
