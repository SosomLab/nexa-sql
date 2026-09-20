# 63a — 변수 관리 조사 원문 (영문 · 09-21)

> [63 변수 관리](63-variable-management.md)의 근거 자료. 하위 조사 7건(오픈 소스는 소스 수준 · usql은 빌드·실행)을 모은 것 — 주장마다 [V] 확인 · [I] 추론 · [M] 기억 표시. 설계 결론은 63이 SSOT이고 이 문서는 고치지 않는다(다시 조사하면 새 절을 덧붙인다).


Legend: **[V]** verified this session against the cited doc page or source file (by one of seven research passes) · **[I]** inference · **[M]** from memory, not verified. Source line numbers refer to the HEADs cloned on 2026-09-20/21 (postgres `9e17d25`, dbeaver `2272b01`, usql `14636a8`, go-sqlcmd `b359e41`, tiberius `feb8df2`, SQL Workbench/J codeberg `18de60d`, HeidiSQL `aa0f350`, Beekeeper `ee158d0`, DbGate `65c2f25`, SQLTools `e9a2491`, SQuirreL `f69fc9e`, Jailer `b759f44`, sqlite `shell.c.in` master).

Context: nexa-sql already has `crates/nsql-script` (`vars.rs` VarStore, `bind.rs` extract_binds, `dialect.rs` prepare/wrap_exec/rewrite_select_into_tsql, `engine.rs` Action/absorb) and docs/08 (DR-8 "variables live in the client"). The Oracle driver already binds `OracleType::RefCursor` and reads `stmt.implicit_result()`. This report is therefore written as *gap analysis + universal model*, not a green-field design.

Corrections to premises in the brief (all [V]):
1. Real SQL*Plus does **not** auto-declare binds — `EXEC :V := 'x'` without `VAR` gives `SP2-0552`. The implicit declaration the user relies on is **Benthic Golden** behaviour: "Golden will autodefine any bind variables it sees in your statements as string type and give them an initial value of """.
2. psql `\bind_named`, `\parse`, `\close_prepared` are **PG18** (not 17; and the name is `\close_prepared`). `\bind` is PG16.
3. psql docs have no "array slice needs a space" rule. The real rules: *undefined variables are left untouched* + `\:` escape.
4. DataGrip's default user-parameter patterns do **not** include `:name` / `@name`.
5. DBeaver does **not** bind — it substitutes text; and its Oracle support returns only the first implicit result.
6. MongoDB *does* have a server-side variable mechanism (`let` + `$$var`, only inside `$expr`).
7. The SQL*Plus VARIABLE page still says binds "cannot be used in … SQL statements, except in PL/SQL blocks"; the sibling page shows `select :abc from dual`. The latter is the real behaviour.

---

## 0. Taxonomy — everything variables are used for

Each entry: what · example · who has it. "—" = found in no tool surveyed (a gap = differentiation opportunity).

### 0.1 Declaration & typing
| Use | Example | Tools |
|---|---|---|
| Explicit typed declaration | `VAR n NUMBER` · `VAR s VARCHAR2(30)='Smith'` (12.2+) | SQL*Plus/SQLcl, PL/SQL Dev Command Window, Toad F5, Golden (`var x STRING`), nsql-script |
| Implicit declaration on first sight | `EXEC :V := 'x'` with no VAR | **Golden** (auto string, `""`), PL/SQL Dev Test Window ("Scan source", auto = String), nsql-script (`Auto`) |
| Type set in a grid/dialog | type dropdown per bind | Toad bind dialog (incl. Cursor), PL/SQL Dev Test Window (Integer/Float/String/Date/Long/Long Raw/Cursor/CLOB/BLOB/BFile/PL-SQL String/Char/Substitution/Temp LOB), DynamoDB NoSQL Workbench, Metabase/Redash widgets |
| Untyped text only | `\set x 1` · `:setvar x 1` · `@set x = 1` · `WbVarDef x=1` | psql, usql, sqlcmd, DBeaver, SQL Workbench/J, HeidiSQL, Beekeeper, DbGate, SQLTools, Jailer, TablePlus |
| Dynamic storage-class typing | `.parameter set :n 42` (value evaluated as SQL expr) | sqlite3 shell |
| JSON-valued | `\SET -$city "Paris";` · `:param x => {a:1}` | Couchbase cbq/Workbench, Neo4j, Cosmos, Kibana Console |
| SQL*Plus type list (23ai) [V] | NUMBER, CHAR, NCHAR, VARCHAR2, NVARCHAR2, CLOB, NCLOB, REFCURSOR, BINARY_FLOAT, BINARY_DOUBLE, BOOLEAN, VECTOR — **no DATE, no BLOB** | docs.oracle.com/…/23/sqpug/VARIABLE.html |

### 0.2 Assignment
| Use | Example | Tools |
|---|---|---|
| Literal, no round trip | `VAR x = Smith` · `\set x 1` · nsql `EXEC :V := 'lit'` | all |
| From server expression | `EXEC :n := f(1)` · `.parameter set :b ":a+1"` · `:param x => datetime()` (server-evaluated) | SQL*Plus, sqlite3, Neo4j |
| One row → many vars | `EXEC SELECT a,b INTO :A,:B FROM t` (valid: EXEC = `BEGIN…END;`) · `SELECT a,b \gset p_` · `WbVarDef -variable=id,name -query="…"` · `SELECT … INTO @a,@b` · `SELECT @a=col` | SQL*Plus/Golden, psql/usql, SQL Workbench/J, MySQL, T-SQL |
| Column header → substitution var | `COLUMN c NEW_VALUE v` | SQL*Plus, PL/SQL Dev Cmd Window, Jailer (last row wins) |
| Row-count policy | psql: 0 rows = error, >1 = error, **NULL unsets**; WbVarDef: first row, `-nullHandling=empty|ignore|remove`; MySQL: >1 = error 1172, **0 rows = warning 1329 and value unchanged**; T-SQL `SELECT @v=col`: **last row, no error**; Oracle: NO_DATA_FOUND / TOO_MANY_ROWS | — |
| Column → list/array var | `IN (&<name multiselect="yes" list="select…">)` | PL/SQL Dev; Grafana multi-value; otherwise — |
| Whole result set → variable | `result = %sql …` / `<<` | JupySQL/ipython-sql; REFCURSOR is the Oracle analogue; T-SQL table variables/TVP server-side; otherwise — |
| Query *text* in a variable | DBeaver "Assign variable with query text"; WbVarDef `-contentFile`, `$[my_select];` | DBeaver, SQL Workbench/J |

### 0.3 Binds
| Use | Example | Tools |
|---|---|---|
| Real typed IN bind | `:v` sent via OCIBindByName | SQL*Plus, SQLcl, Toad, PL/SQL Dev, Golden; sqlite3 shell (`bind_prepared_stmt`); psql `\bind` (text, untyped OIDs); usql `\bind`; SQL Workbench/J `?` (opt-in `workbench.sql.checkprepared`); cbq; Neo4j; Metabase; ipython-sql |
| "Bind" that is really text replace | `:p` → text | **DBeaver** (`SQLUtils.fillQueryParameters`, plain `createStatement`; open request #15310), DataGrip, HeidiSQL, Beekeeper (via sql-formatter!), DbGate, SQLTools, TablePlus, SQuirreL sqlparam, Navicat, Redash, Superset |
| OUT / INOUT bind | `EXEC p(:out)` | Oracle tools (all placeholders in PL/SQL are at least IN — OCI doc); SQL Workbench/J `WbCall p(?, ?)`; DBeaver only via `CALL …(?)` → `JDBCCallableStatementImpl` |
| Function return value | `EXEC :rc := f()` · `{? = call f}` · T-SQL `EXEC @rc = p` | SQL*Plus, WbCall, SSMS-generated script |
| DML RETURNING | `… RETURNING id INTO :id` | Oracle tools (rust-oracle `returned_values`); PG/MariaDB/SQLite return rows instead |
| Array / bulk bind | OCI array DML, JDBC executeBatch, MariaDB `COM_STMT_BULK_EXECUTE`, PG `unnest($1::int[])`, MSSQL TVP | driver-level only; **no interactive client offers "run once per row of a CSV/result"** (closest: `GO n`, psql `\gexec`) — gap |

### 0.4 Result streams
| Use | Example | Tools |
|---|---|---|
| REF CURSOR out → grid | `VAR rc REFCURSOR; EXEC p(:rc); PRINT rc` | SQL*Plus (text; PRINT **closes** the cursor, one-shot), Golden (**grid**; Results ▸ Bind Variable Cursors; magic name `:cursor` needs no VAR), Toad (type = Cursor → data grid), PL/SQL Dev (Cursor cell button → SQL Window), SQL Developer (Run PL/SQL ▸ Output Variables tab, first 100 rows), **WbCall: one result tab per cursor, tab named after the parameter**, DataGrip 2023.3 (navigate into cursor cell) |
| Autoprint | `SET AUTOPRINT ON` prints binds referenced by a successful block | SQL*Plus, PL/SQL Dev Cmd Window |
| Implicit results | `DBMS_SQL.RETURN_RESULT` → "ResultSet #1…" | SQL*Plus 12.1+, SQL Developer 4.1+; DBeaver only first (#17451 open, `supportsMultipleResults()` false for Oracle) |
| Cursor as a cell value | `SELECT CURSOR(…)`, PG refcursor | DBeaver `DBDCursor` (`OracleRefCursor`, `PostgreRefCursor`: `MOVE ABSOLUTE 0 IN "n"` + `FETCH ALL IN "n"`, `CLOSE` on release, **throws in autocommit**); pgJDBC `getObject` → `FETCH ALL IN`; psql & pgAdmin print only the name |
| Multiple result sets → tabs | proc with N SELECTs | SSMS, HeidiSQL, Workbench, DBeaver (loop `nextResults()`), Navicat (OUT params in their own tab), psql 15+ `SHOW_ALL_RESULTS`; **pgAdmin shows only the last** [I from code] |
| OUT scalars shown as a synthetic result | `PARAMETER | VALUE` grid | WbCall; DBeaver `JDBCResultSetCallable`; MySQL binary protocol does this natively (`SERVER_PS_OUT_PARAMS`) |
| Table functions | `SELECT * FROM TABLE(f(:x))` (TABLE optional 12.2+), `dbo.f(@p)`, PG SRF, `json_each(:j)`, `pragma_table_info(:t)` | plain query + binds; nothing special needed |

### 0.5 Substitution (text macro)
| Use | Example | Tools |
|---|---|---|
| Plain | `&v` `&&v` `DEFINE` · `:v` (psql) · `$(v)` · `${v}` · `$[v]` · `[$v]` · `{{v}}` | SQL*Plus family, psql/usql, sqlcmd/SSMS/ADS, DBeaver/DataGrip/TablePlus/Kibana, SQL Workbench/J, Navicat, Metabase/Redash/JupySQL |
| Safe-quoted forms | `:'v'` literal (PQescapeLiteral) · `:"v"` identifier · `:{?v}` defined-test | **psql only** (usql has the syntax but **does not escape** — verified bug) |
| Format specifiers for lists/languages | `${v:csv}` `:sqlstring` `:singlequote` `:json` `:regex` `:lucene` | Grafana |
| Auto-quote by declared type | `&<name="x" type="string">` | PL/SQL Developer |
| Identifiers / object names / fragments | `SELECT * FROM &tab` · `ORDER BY &<… prefix="order by ">` | every macro tool; binds can never do this (ORA-01027 for DDL; Neo4j labels; Cassandra tables). Backends with *identifier placeholders*: ES|QL `??name`, DynamoDB `#name`, Redis KEYS[] |
| Concatenation terminator / escape | `&f..log` (SET CONCAT) · `SET ESCAPE \` · `SET DEFINE OFF` · psql `\:` · PL/SQL Dev `&&` = literal & (!) | — |
| Inside strings/comments? | SQL*Plus: **yes both**; sqlcmd: strings yes, comments no; DBeaver `${}`: yes both, `:p`: no; WbJ: yes both; psql: **no** (INITIAL lexer state only); nsql-script: not in comments | — |

### 0.6 Prompting
| Use | Example | Tools |
|---|---|---|
| Scripted prompt | `ACCEPT v NUMBER FORMAT … DEFAULT … PROMPT '…' HIDE` · `\prompt 'text' v` · `ARGUMENT 1 PROMPT … DEFAULT …` (SQL*Plus 23/SQLcl 22.4) · DBeaver PRO `@accept` | — |
| Auto prompt for undefined | `&v`; `$[?v]` always / `$[&v]` only-if-undefined; strategy `workbench.sql.parameter.prompt.strategy`; DBeaver dialog skipped when all set, "Hide parameters set in script", **Ignore** button; sqlcmd: none (error text, sends `$(x)` as is) | — |
| Pick-lists, query-fed, cascading, multiselect, checkbox, hidden, required, default-from-query | `&<name= list="select…" description="yes" restricted="yes" multiselect="yes">`; WbJ `-values='a,b,c'`; Redash query-based dropdown | **PL/SQL Developer (richest)**, WbJ, Redash/Metabase |
| Value history | DBeaver global `parameter-bindings.xml` by name; DataGrip last value in memory only (history = open request); HeidiSQL per tab in `tabs.ini`; DbGate per tab localStorage; SQuirreL per-plugin cache, "hide text" = not cached | — |
| Password input | `ACCEPT … HIDE`, `\password` (psql `\prompt` has no hide) | SQL*Plus, psql |

### 0.7 Predefined / system variables
SQL*Plus `_USER _CONNECT_IDENTIFIER _DATE _EDITOR _O_VERSION _O_RELEASE _PRIVILEGE _SQLPLUS_RELEASE` (+ `_SQL_ID` in 26ai docs), `SQL.SQLCODE/PNO/LNO/USER/RELEASE` · psql `ROW_COUNT ERROR SQLSTATE LAST_ERROR_MESSAGE LAST_ERROR_SQLSTATE LASTOID DBNAME USER HOST PORT SERVER_VERSION_NUM SHELL_ERROR SHELL_EXIT_CODE …` · sqlcmd `SQLCMDUSER/SERVER/DBNAME/WORKSTATION/ERRORLEVEL/COLSEP…` (read-only set) · DBeaver `${host} ${database} ${user} ${date} ${time} ${file} ${workspace}…` · Golden pseudo-vars (tab name, file name, date/time). Elapsed time as a variable: —. Last-insert-id: server functions only.

### 0.8 Environment & arguments
`@script a b` → `&1 &2` · psql `-v`, `\getenv`, `\setenv`, backticks · sqlcmd `-v`, env fallback (precedence sys env < user env < shell < `-v` < `:setvar`), `-x` disables substitution · WbJ `-varFile`, `-variable`, `wbp.*` system props · usql `--set` · cypher-shell `-P` · ADS/sqltoolsservice `$(VAR)` falls back to env.

### 0.9 Flow control
psql `\if :ok … \elif … \endif` (no expansion in skipped branches), `\gexec`, `\watch` · SQL*Plus `WHENEVER SQLERROR EXIT SQL.SQLCODE ROLLBACK`, NEW_VALUE + `@&script` idiom [M] · sqlcmd `:on error exit|ignore`, `:EXIT(query)`, `GO n` · WbJ `-ifDefined/-ifNotDefined/-ifEquals/-ifEmpty` on WbInclude/WbVarDef · SQLcl `SCRIPT` JS, `REPEAT` [M] · cbq `\PUSH/\POP` per-variable stacks for nested `\SOURCE`. DBeaver: none.

### 0.10 Variables in client commands
`SPOOL &f..log` · `CONNECT &u/&p@&db` [M] · `\o :file` `\i :file` (not `\copy`) · `:connect $(server)` `:out $(dir)` · `WbExport -file=$[dir]/x.csv` · usql `\copy :SRC :DST`.

### 0.11 Formatting
`COLUMN … NEW_VALUE/OLD_VALUE … NOPRINT` with TTITLE/BTITLE; PRINT honours COLUMN formats; **no DATE bind in SQL*Plus → dates travel as strings and depend on NLS_DATE_FORMAT**; psql `\pset null`.

### 0.12 Scope, persistence, sharing
| Scope | Tools |
|---|---|
| Process-global | SQL*Plus (one name space, survives CONNECT and `@` sub-scripts [V for substitution; I for binds]), psql, sqlcmd (survives `:connect`), Jailer, WbJ default |
| Per window/tab | **Golden (per tab, both kinds)**, WbJ option "Separate variables per window", HeidiSQL, DbGate, Beekeeper, PL/SQL Dev Test Window (`.tst` stores block + name/type/value + watches) |
| Per connection | sqlite3 (`temp.sqlite_parameters`, lost on `.open`), Neo4j Browser (per DBMS, optional local storage), WbJ *profile* variables (defined on connect, removed on disconnect) |
| Layered | WbJ: command line > workspace > profile |
| Per driver, on disk | DBeaver `sql-variables-driver-<id>.json` (all editors of that driver share) + global `parameter-bindings.xml` |
| dev/test/prod variable environments (Postman-style) | — (gap; nexa-sql already has `ConnectSpec.env`) |
| Import/export | WbJ properties files; PL/SQL Dev "export as SQL*Plus script"; `login.sql`, `.psqlrc`, `SQLCMDINI`, `.usqlrc` |

### 0.13 Secrets
`ACCEPT HIDE`; psql `\password`; SQuirreL "hide text" = not cached; SQLcl `SET SECURELITERALS`; usql `.usqlpass` 0600. **No tool refuses to persist password-looking values or masks them in an inspector/log** — gap.

### 0.14 Inspector UI
Commands: `DEFINE`, `VARIABLE`, `PRINT`, `\set`, `:listvar`, `.parameter list`, `WbVarList` (**editable result grid**: add row = create, delete row = drop, `WbVarList foo*`), cbq `\SET;`, Neo4j `:params`. Panels: DBeaver Variables panel (Variable/Value/Type, edit below, commit on blur, "Show parameters"), DBeaver bind dialog (substituted-SQL **preview**), PL/SQL Dev grid (changed values highlighted yellow, vars not in source shown disabled rather than deleted, LOB file import/export), Toad dialog, SQL Developer "Enter Binds" (everything VARCHAR2, NULL checkbox), Couchbase Run-Time Preferences, Kibana Variables tab. Hover-shows-value / rename / undefined-variable lint: — (gap).

### 0.15 Debugging & output capture
PL/SQL debuggers (PL/SQL Dev Set Variable/watches saved in `.tst`; SQL Developer Smart Data/Watches; DataGrip) · SSMS T-SQL debugger removed in 18.0 · DBMS_OUTPUT panes (DBeaver `OracleOutputReader`), T-SQL PRINT/RAISERROR info tokens, PG NoticeResponse.

### 0.16 Result ↔ variable round trips
PL/SQL Dev Linked Query: implicit binds `:m_<master_field>` re-query detail on master row change · SQL Developer child reports bound by `:COLUMN_NAME` (uppercase) · templates: SSMS `<name, type, default>`, DBeaver `${table}`, pgAdmin macro `$SELECTION$`, DbGate "CALL OBJECT" template with `:` params · dashboards: Metabase/Redash/Grafana/Superset. "Send this grid cell to variable X": — (gap).

### 0.17 Transactions
Client-held variables are non-transactional (values survive ROLLBACK). T-SQL locals/table variables: not rolled back [V]. MySQL `@v`: non-transactional [M]. **PG custom GUC `SET` is rolled back on abort; `SET LOCAL` ends with the transaction** [V] — so GUC-emulated variables are transactional, as are temp-table emulations.

### 0.18 Logging / audit
PG `log_parameter_max_length` · Oracle `V$SQL_BIND_CAPTURE` (simple types, WHERE/HAVING only, ≤ every 15 min, 4000 chars) · DBeaver Query Manager logs post-substitution SQL · rusqlite `expanded_sql()` · psql `HISTCONTROL ignorespace`.

### 0.19 NoSQL analogues
Session state = language variables (mongosh JS REPL) · named params as JSON (`$name` Couchbase/Neo4j, `@name` Cosmos, `?name` ES|QL, `:name` Cassandra prepared, `params.x` Flux Cloud) · positional only (DynamoDB PartiQL `?`, ES SQL `?`, Redis ARGV) · identifier placeholders (ES|QL `??`, DynamoDB `#n`, Redis KEYS) · server text templates (ES mustache search templates) · client text variables (Kibana `${v}` — `"${v}"` **strips the quotes** to inject numbers/objects, `"""${v}"""` forces string; stored in localStorage) · no variables at all (redis-cli, cqlsh, Compass) · result streams = cursor id + getMore (Mongo 101 docs/16 MiB first batch), paging_state (Cassandra), NextToken (DynamoDB), continuation token (Cosmos), Bolt PULL n, SCAN cursor · third state besides NULL: Cassandra **unset (-2)**, MariaDB bulk **DEFAULT/IGNORE** indicators, Mongo/Couchbase/Cosmos **missing/undefined ≠ null**.

---

## 1. Per-client findings

### 1.1 SQL*Plus / SQLcl
- Two name spaces: substitution (text, pre-lexical — replaced even inside comments and string literals; types CHAR/NUMBER/BINARY_*; max 2048; CHAR ≤ 240 bytes; "just one global name space … if you reconnect using CONNECT, or run subscripts using @, all variables ever defined are available") and bind (typed, client memory). docs.oracle.com/en/database/oracle/oracle-database/23/sqpug/using-substitution-variables-sqlplus.html [V]
- `VAR[IABLE] [variable [type [=value]]]`; `= value` added in 12.2 ("Input Binding with VARIABLE Command") [V].
- `EXECUTE` = "Executes a single PL/SQL statement"; the BEGIN…END wrapping is community-documented (orafaq t/163453), so `EXEC SELECT … INTO :a,:b` is legal [I, strong].
- Undeclared bind → `SP2-0552` (client-side scan) [V]. SQL*Plus does not scan `:new` inside a well-formed `CREATE TRIGGER`, but reports `SP2-0552: Bind variable "NEW"` as soon as the TRIGGER keyword is missing (orafaq t/187510) → the scan is gated on statement class [I].
- PRINT closes a REFCURSOR; "cannot be PRINTed more than once" [V]. AUTOPRINT [V]. Implicit results print as "ResultSet #n" [V].
- Binds not allowed in DDL: ORA-01027 [V]. Names: effectively case-insensitive (ODPI-C/rust-oracle upper-case bind names) [V for driver, I for SQL*Plus].
- SQLcl: supports VARIABLE/PRINT/DEFINE (not on the unsupported list) [V]; ALIAS with `:name` args; `SCRIPT` JS `util.executeReturnList(sql, binds)`; `SET SECURELITERALS`; no `BIND` command found.

### 1.2 SQL Developer
F9 statement runner: "Enter Binds" dialog, all values sent as VARCHAR2 → peeked bind shows `VARCHAR2(30)` where SQL*Plus shows NUMBER, **changing CBO estimates/plans** (jonathanlewis.wordpress.com/2025/09/24/sql-developer-and-cbo/) [V]. F5 script runner: VAR/EXEC/PRINT work, binds never prompted [V]. REF CURSOR: Run PL/SQL ▸ Output Variables (100 rows, no sort/export) or F5 text (thatjeffsmith.com …viewing-refcursor-output) [V]. Pitfalls: F9 on CREATE TRIGGER prompts for `:NEW/:OLD`; non-Oracle `c:a1` treated as bind; `&` in strings needs `SET DEFINE OFF` [V].

### 1.3 PL/SQL Developer (manual 15.0, §5, §7.5, §8.3, §12.3) [V]
Test Window variable grid (see 0.14); auto-scan adds unknown binds as String; Boolean impossible ("SQL*Net does not support") → Integer + `sys.diutil.bool_to_int`; a **"Substitution" variable type inside the same grid** (text-replaced "without the restriction of bind variables"); `.tst` persistence. SQL Window `&<…>` rich substitution; from 16.0 `:bind` in SQL Window with the same attribute syntax. Command Window = SQL*Plus subset. Session queries: "bind variables will be used if the query is a PL/SQL Block or DML statement, whereas substitution variables will be used for other statements such as ALTER SESSION" — automatic bind→text downgrade by statement class.

### 1.4 Toad
F9: Bind Variables dialog with datatype incl. **Cursor** → grid. F5 = SQL*Plus emulator: binds never prompted, needs VARIABLE/PRINT (forums.toadworld.com t/30987, t/55985) [V]. Value memory / `:NEW` handling [M].

### 1.5 Benthic Golden (help PDF is 3.x-era; product page current) [V]
Per-tab lists for both prompt (`&`) and bind variables (Script ▸ Variables). Types INTEGER, NUMBER, STRING(4000), PLSQLSTRING(32K), REFCURSOR. Auto-define as string. `var name = "Data"`, `var clearall`, bare `var`/`print` list (`MYVAR1 STRING = …`). REFCURSOR → spreadsheet grid, one visible at a time, Results ▸ Bind Variable Cursors; the name `:cursor` works undeclared. OUT args/function results displayed after EXEC.

### 1.6 DBeaver (source-verified)
- One class `SQLQueryParameter` (String value) for `:name`, `?` (off by default) and `${var}`; prefs in `ModelPreferences` (`sql.parameter.enabled/prefix/mark/anonymous.enabled/ddl.enabled`, `sql.variables.enabled`). PG adds `$` prefix; MSSQL `@` prefix was **abandoned** (#5674).
- Text substitution back-to-front by token offset (`SQLUtils.fillQueryParameters`), executed with plain `createStatement()`; empty value → `NULL`. Issues: #1010 quoting, #4844 injection, **#15310 real binds (open)**.
- Two stores in `SQLScriptContext`: `variables` (`@set`, panel) and `defaultParameters` (dialog). Lookup: variables → defaultParameters, so `@set x = 1` silences the prompt for `:x` and `${x}`. Unquoted names upper-cased.
- `@set` stores raw text after `=` (no evaluation). Commands via extension point `org.jkiss.dbeaver.sqlCommand`: set, unset, echo, export, include (+ PRO `@accept`, `@pause`, `@ai`).
- Parser `ScriptParameterRule.evaluate`: rejects when previous char is an identifier char, the prefix itself (kills `::`), `\`, `/`, or `[`; needs ≥1 identifier char after (kills `:=`); strings/comments are consumed by the token scanner first; `${var}` however is regex-replaced **inside strings and comments**. DDL (first keyword CREATE/ALTER/DROP) and `$$` blocks skip parameters unless `sql.parameter.ddl.enabled`. Anonymous PL/SQL blocks are *not* DDL → `:x` prompted and text-replaced → host OUT binds impossible.
- OUT params: only when `isExecQuery` (Oracle keyword = `call`) **and** the SQL `contains("?")` → `prepareCall`; OUT values surfaced as synthetic `JDBCResultSetCallable`; IN values cannot be bound (#8911, #35477, #38749 open).
- False positives/negatives: #19569 Snowflake `col:path`, #15575 array slice, #7938 `PREPARE … $1`, #5700 MySQL `:=`, #16721 inside `$$`, #37736 `array[:p]`, #22335 `:schema.*`; UX: dialog per statement in a script (#34992, #4887).
- Assign-from-result: none.

### 1.7 DataGrip
Regex pattern list for user parameters with per-pattern scope flags ("In scripts", "In literals"), capture group ⇒ named (asked once) [V jetbrains.com/help/datagrip/settings-tools-database-user-parameters.html]; "the replacement of a parameter with a value is straightforward" (text) [V]; last values kept in memory; no history. Oracle ref cursors in console since 2023.3 [V]. Execute Routine dialog.

### 1.8 psql (source-verified)
- One `VariableSpace`; specials are the same variables with assign/substitute hooks (`variables.c`, `startup.c`). All `char*`.
- Lexer `src/fe_utils/psqlscan.l`: variable rules exist only in INITIAL state (not in xc/xq/xe/xus/xdolq/xd/xui); `typecast "::"` and `colon_equals ":="` are separate tokens consumed first; **undefined variable → echoed unchanged**; unquoted substitution pushes the value as a new buffer and **rescans** it (recursion guarded); quoted forms use `PQescapeLiteral/Identifier`, are not rescanned and need a live connection.
- `\gset`: `StoreQueryTuple` (common.c): exactly one row, prefix + column name, NULL ⇒ unset, refuses hooked specials. `\gdesc` = describe without executing (`PQdescribePrepared`). `\bind` = `PQsendQueryParams` with NULL type OIDs, one-shot.
- Inactive `\if` branch disables substitution. refcursor: zero handling (prints the portal name).

### 1.9 usql (source + built and run)
Three stores (`\set`, `\pset`, `\cset`). **Undefined `:v` is left for the driver**, so Oracle/SQLite native binds work — but defining a same-named variable silently turns the bind into text. Verified defects: `1::text` with `\set text X` → `1:X`; `:'v'` adds quotes **without escaping**; substituted text is rescanned from i+1 and can swallow following statements; 0-row `\gset` panics; NULL → empty string; splits at every `;` (breaks `begin :v := 1; end;`); no T-SQL `GO`. `\bind` passes strings to `QueryContext`. No `sql.Out`/refcursor handling.

### 1.10 pgAdmin / pgcli / mycli
pgAdmin: no variables (macro `$SELECTION$` only), no refcursor fetch, last result only [V code]. pgcli/mycli: named/favorite queries with `$1`, `$*`, `$@` via raw `str.replace` (so `$1` eats the head of `$10`; collides with native PG `$1`).

### 1.11 sqlcmd / go-sqlcmd / SSMS / ADS
`Variables map[string]string`, upper-cased keys, read-only set, env fallback (`pkg/sqlcmd/variables.go`). `$(v)` positions recorded while scanning (`batch.go`), **also inside string/bracket/quoted literals, not in comments**; substituted late at `GO` (`getRunnableQuery`) so a `:setvar` higher in the same batch applies. Undefined → message, `$(x)` sent as is. No quoting form, no assign-from-query, no OUT. SSMS SQLCMD mode: variables **case-sensitive** (opposite of sqlcmd), no `:listvar`, IntelliSense off. sqltoolsservice (ADS/vscode-mssql): `:setvar`, `:r`, `:connect`, `:on error`, `GO`; the rest are `UnsupportedCommand`. SSMS template parameters `<name, type, default>` = one-shot text edit. SSMS Execute Procedure generates `DECLARE @return_value int; EXEC @return_value = p @in=…, @out=@out OUTPUT; SELECT @out; SELECT 'Return Value'=@return_value`.

### 1.12 mysql / Workbench / HeidiSQL
mysql CLI: no client variables. Workbench `ExecuteRoutineWizard::run()` generates `set @p = 0; call s.p(…, @p); select @p;` with unescaped literals. HeidiSQL: regex `([^:\w]|^):\w+` over the whole memo (strings/comments not excluded, disabled above 1 MB), descending-name `StringReplace`, empty value = not replaced, per-tab, persisted; dev quote: "Parameter binding is done in HeidiSQL itself".

### 1.13 sqlite3 shell
`temp.sqlite_parameters(key TEXT PRIMARY KEY, value) WITHOUT ROWID`; `bind_prepared_stmt` loops `sqlite3_bind_parameter_name` (unnamed → synthesised `?N` key — source and docs disagree), `sqlite3_bind_value` keeps the storage class, missing → NULL. `.parameter set NAME VALUE` evaluates VALUE as SQL (sub-queries and other parameters allowed), falls back to text — hence the documented `"'202-456-1111'"` pitfall. Key includes the prefix (`:x` ≠ `@x` ≠ `$x`). **The engine reports parameter names after prepare — no client parser needed** (rusqlite `parameter_count/parameter_name/raw_bind_parameter/expanded_sql`).

### 1.14 SQL Workbench/J (manual + source) — the best cross-DB reference
- `VariablePool`: case-insensitive `Map<String,String>`, names `[\w\.]+`, regex-only replacement **inside strings and comments**, recursive expansion with self-reference guard. Manual: if `$[`…`]` is needed in real SQL, define no variables — "unpredictable results".
- Scopes: global (default) / per-window option / profile vars (connect…disconnect) / workspace vars; priority cmdline > workspace > profile.
- `WbVarDef -variable=a,b -query="…"` (first row; extra columns ignored; extra vars undefined; `-nullHandling`), `-file`, `-contentFile`, `-values` pick-list, `$[?v]`/`$[&v]`, `WbVarList` editable grid, `-ifDefined…` conditionals.
- `WbCall` (`wbcommands/WbCall.java`): rewrites to `{call …}`/`{? = call …}`, parameter metadata from `getParameterMetaData()` with fallback to `getProcedureColumns()` (`workbench.db.[dbid].parameter.metadata.callablestatement.supported`), `?` in an IN slot prompts a typed dialog, OUT scalars → `PARAMETER|VALUE` grid, **each REF CURSOR → its own result tab named after the parameter**, PG refcursor functions handled; `EXEC`/`EXECUTE` verbs map to WbCall (`CommandMapper.java`).
- Real `?` prepared statements only with `workbench.sql.checkprepared=true` (default off; self-disables on driver metadata errors).

### 1.15 Beekeeper / DbGate / SQLTools / TablePlus / Navicat / SQuirreL / Jailer
All text replacement. Beekeeper substitutes through **sql-formatter** (executed SQL gets reformatted) — issues #391 (`:1` inside a JSON string), #2560 (`geometry::STGeomFromText`), #581 (a bcrypt value `$2y$10$` re-parsed as params), #3162 "should just work" (open). DbGate #925 (`::date`) was "fixed" by making the default style **"(no parameters)"**; its scanner shares the splitter's tokenizer (strings/comments first). SQLTools: opt-in regex, per-connection `variables`, `-- @var name = value` comment directives. TablePlus: opt-in, pick one of four regexes. SQuirreL sqlparam: regex `[\ \(]:[a-zA-Z]\w+` (misses `=:x`), numeric-or-auto-quote, "hide text" not cached. Jailer: SQL*Plus subset incl. `COLUMN … NEW_VALUE` (last row wins).

### 1.16 NoSQL / BI clients — see 0.19 and §3. Highlights
- **cbq**: `\SET -$name <json>;`, `-args [...]`, per-variable stack with `\PUSH/\POP/\UNSET`, values are JSON sent as real request parameters.
- **Neo4j**: `:param x => expr` is evaluated **by the server** as Cypher (type-faithful); `:params {x:1}` makes integers floats (documented pitfall); no params for labels/types/property keys.
- **Metabase**: real prepared-statement args (`substitution.clj` builds `?` + `:prepared-statement-args`); `[[ AND x = {{x}} ]]` optional clauses. **Redash**: mustache text; `is_safe` = "no Text-type parameter" gates sharing — *only validated types may be text-substituted*.
- **ipython-sql/JupySQL**: `:x` binds from the Python namespace vs `{{x}}` Jinja text; `result << SELECT …`.

---

## 2. Comparison matrix (rows follow the taxonomy)

B = real bind · T = text · – = none. Scope: G global/process, W window/tab, C connection, D on disk.

| Tool | Typed decl | Bind kind | Macro kind | Safe quoting | Assign from query | OUT/return | Cursor / multi-result UI | Prompt richness | System vars | Flow control | Scope / persist | Inspector | `:name` disambiguation | Signature pitfall |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| SQL*Plus/SQLcl | yes (no DATE) | B named | `&` `&&` | – | INTO binds, NEW_VALUE | B | PRINT text, autoprint, implicit results | ACCEPT (type, format, default, hide) | rich | WHENEVER | G, login.sql | DEFINE/VAR/PRINT | statement-class gate | `&` in strings/comments; one-shot cursor |
| SQL Developer | F5 only | B but all VARCHAR2 | `&` | – | F5 only | Run dialog | Output Variables (100 rows) | dialog + NULL box | – | F5 subset | session | dialog | weak (trigger `:NEW`) | plan differs from app |
| PL/SQL Developer | grid types | B | `&<…>` + "Substitution" type | by `type=` | INTO binds | B | Cursor cell → SQL Window | **richest** | – | Cmd Window | W, `.tst` on disk | **best grid** | scan, disable-not-delete | `&&` means literal & |
| Toad | dialog types | B (F9) | `&` | – | INTO binds | B | Cursor type → grid | dialog | – | F5 | session [M] | dialog | [M] | F5 never prompts binds |
| Golden | yes + **auto** | B | `&` `&&` | – | INTO binds | B, shown after EXEC | **grid, cursor switcher** | prompt | pseudo-vars | script | **W (per tab)** | `var`/`print`, menu | – | one cursor visible at a time |
| DBeaver | – | T (`:p`, `?`) | `${v}` | – | – | only `CALL(?)` synthetic RS | cursor cell viewer; tabs per RS; Oracle implicit: first only | dialog + preview + Ignore | `${host}`… | – | editor→driver file D; global bindings D | Variables panel | token rule + DDL/`$$` gate | no real binds (#15310) |
| DataGrip | – | T | regex patterns | – | – | Execute Routine | cursor navigation (2023.3) | inline table | – | – | memory | View Parameters | regex + scope flags | no history |
| psql | – | `\bind` B (text) | `:v` | **`:'v'` `:"v"`** | `\gset` | rows only | name only | `\prompt` | **richest** | `\if` `\gexec` `\watch` | G, .psqlrc | `\set` | **lexer states; undefined untouched** | `:v` macro is unsafe by design |
| usql | – | `\bind` | `:v` | broken | `\gset` (panics on 0 rows) | – | multi RS | `\prompt -TYPE` | few | `\if` | G | `\set` | hand scanner, `::` bug | defined var hijacks native bind |
| sqlcmd/SSMS/ADS | – | – | `$(v)` | – | – | – | multi RS | – | SQLCMD* | `:on error`, `GO n` | G, env | `:listvar` | `$(` also in strings | case rules differ SSMS vs sqlcmd |
| sqlite3 | dynamic | **B by engine names** | – | n/a | `.param set` expr | – | – | – | `$TIMER` etc. | – | C (temp table) | `.param list` | engine | prefix part of key; value evaluated |
| MySQL Workbench/mysql | – | – | – | – | server `@v` | generated script | tabs | routine dialog | – | – | server session | – | – | unescaped literals |
| HeidiSQL | – | T | – | – | – | – | tabs | tree node | – | – | W + D | helper tree | one regex | strings/comments not excluded |
| SQL Workbench/J | `?` typed (opt-in) | B (`?`, WbCall) | `$[v]` | – | **`-query` multi-var** | **WbCall grid** | **tab per ref cursor** | `$[?v]`, pick-lists, strategy | – | `-ifDefined…` | G / W / profile / workspace | **WbVarList editable grid** | distinct delimiter | replaces in comments/strings |
| Beekeeper/DbGate/SQLTools/TablePlus | – | T | – | – | – | – | tabs | modal | – | – | W (DbGate D) | modal | regex / shared tokenizer | `::`, JSON-in-string; mostly opt-in now |
| cbq / Couchbase WB | JSON | **B** | – | n/a | – | – | JSON stream | prefs UI | predefined | `\PUSH/\POP` | G stack | `\SET;` | engine | – |
| Neo4j Browser/shell | Cypher types | **B** | – | n/a | `:param x => expr` server-eval | – | stream | missing-param hint | – | – | C, optional D | `:params` | engine | int→float in `:params {}` |
| Kibana Console | JSON-ish | – | `${v}` | quote-stripping rule | – | – | – | – | – | – | browser D | Variables tab | – | string-looks-like-number |
| Metabase / Redash | widget types | B / T | `{{v}}` | type-gated (Redash) | query-fed dropdown | – | – | **typed widgets** | few | `[[…]]` | saved query | widgets | template | Redash text = injection |

---

## 3. Capability matrix (DBMS + NoSQL)

| Backend | Client-held typed var feasible via | IN bind | OUT/INOUT | Cursor out / multi-result | Assign from query | Table functions | Server-side session-variable alternative |
|---|---|---|---|---|---|---|---|
| **Oracle** | name binds (OCIBindByName; one call binds all same-name slots; PL/SQL position = nth *unique* name) | named, typed | yes — every PL/SQL placeholder is ≥ IN; OUT strings need a max buffer | REF CURSOR out bind (`SQLT_RSET`), implicit results 12.1+ (`dpiStmt_getImplicitResult`; invalid once parent closes), nested `CURSOR()` | `SELECT … INTO :a` in a block; `RETURNING INTO` | pipelined / `TABLE()` (keyword optional 12.2+) | package state, `DBMS_SESSION.SET_CONTEXT` [M] — not needed |
| **SQL Server** | `sp_executesql` with declared params | named `@p`, typed | OUTPUT params + RETURNSTATUS in TDS (`fByRefValue`, RETURNVALUE 0xAC) — **tiberius cannot send ByRef or named RPC (`todo!()`), and drops ReturnValue/ReturnStatus tokens** | many result sets; cursor OUTPUT params "can't be called from the database APIs" | `SELECT @a = col` (last row wins, 0 rows = unchanged) | inline/multi-statement TVF; TVP in | `SESSION_CONTEXT` (sql_variant ≤ 8000 B, 1 MB total, not enumerable), `CONTEXT_INFO` 128 B, `#temp`; locals die at `GO` and are invisible inside `sp_executesql` |
| **PostgreSQL** | `$n` rewrite | positional, type OID optional (unknown → inferred; `could not determine data type of parameter $1`) | none in protocol; OUT/INOUT come back as **one result row** (`CALL`: INOUT PG11, OUT PG14); `void` OID makes an OUT slot ignored | refcursor = name → needs open txn + `FETCH ALL IN "n"`; simple-query multi-statement = multiple results; one statement per Parse | row → vars (client side, `\gset` model) | SRF / `RETURNS TABLE` | custom GUC `set_config('app.v', …)` / `current_setting('app.v', true)` — text only, dotted name, **transactional**; `CREATE VARIABLE`/`LET` still unmerged (commitfest 1608, "Needs review", 2026). `DO` takes no params; utility statements take no params |
| **MySQL** | `?` rewrite or `@v` delegation | positional | text: `CALL p(@o); SELECT @o` · binary PS: OUT/INOUT as an extra one-row result set flagged `SERVER_PS_OUT_PARAMS` | multi result sets (`CLIENT_MULTI_RESULTS`); no client cursors | `SELECT … INTO @a,@b` (>1 row = error 1172; 0 rows = warning, unchanged) | none; `JSON_TABLE` | **`@v`** (session, loses DATE/DECIMAL fidelity), list via `performance_schema.user_variables_by_thread` |
| **MariaDB** | same + `EXECUTE IMMEDIATE … USING <expr>` (10.2.3+; MySQL = user vars only) | positional; **bulk** `COM_STMT_BULK_EXECUTE` with NULL/DEFAULT/IGNORE indicators | same as MySQL | multi result sets; `SYS_REFCURSOR` **12.0.1** (MDEV-20034; OUT params of routines; `max_open_cursors`) — no protocol exposure found [I: server-internal]; `OPEN c FOR '<dynamic>'` 12.3 | same; `INSERT/DELETE/REPLACE … RETURNING` (UPDATE … RETURNING 13.0) | none; JSON_TABLE 10.6; SEQUENCE engine `seq_1_to_10` | `@v` + **`information_schema.USER_VARIABLES`**, `SHOW USER_VARIABLES`, `FLUSH USER_VARIABLES`; `sql_mode=ORACLE` gives PL/SQL-like blocks, `:=`, packages, and `EXECUTE IMMEDIATE '… :x …' USING` (positional despite the names). `SET STATEMENT` is for *system* variables only |
| **SQLite** | engine-reported names | `?`, `?NNN`, `:a`, `@a`, `$a` | none (no procedures) | none | client-side (strip INTO, run, check 1 row) | eponymous vtabs (`json_each`, `pragma_*`, `generate_series` if compiled) | none (temp table / CTE) |
| **MongoDB** | `let` + `$$v` (only in `$expr`/pipelines) else build BSON | named (limited) | – | cursor id + getMore | client | `$lookup` pipelines | – (mongosh JS vars) |
| **Redis** | EVAL/FCALL | ARGV[i] positional, byte strings; KEYS[i] = identifier slot | return value | SCAN cursor | client | – | – |
| **Cassandra/Scylla** | prepared statements | `:name` and `?`; null vs **unset** | LWT `[applied]` row | `paging_state` | client | – | – |
| **Elasticsearch/OpenSearch** | ES|QL `params`, SQL `params`, mustache search templates | `?`, `?1`, `?name`; **identifier `??name`** | – | SQL `cursor`, scroll/PIT [M] | client | – | – |
| **DynamoDB** | PartiQL `Parameters` / classic expression attrs | PartiQL `?`; classic `:val`; **`#name` identifiers** | ReturnValues [M] | NextToken / LastEvaluatedKey | client | – | – |
| **Couchbase** | REST `$name` / `args` | named + positional, JSON values | RETURNING | single JSON stream | client | – | – (cbq client stack) |
| **Neo4j** | Bolt parameters | `$name` map; typed (int ≠ float, temporal, point) | – | PULL n batches [M] | client; server-side expression evaluation of param values | procedures `CALL … YIELD` | – |
| **Cosmos DB** | `parameters:[{name:"@x",value}]` | named JSON | sproc response | continuation token [M] | client | – | – |
| **InfluxDB** | Flux `params` (**Cloud only**; int/float/string) · InfluxQL `$x` (**WHERE only**) | named | – | chunked/CSV stream [M] | client | – | – |

---

## 4. Patterns, anti-patterns, recommended model

### 4.1 Copy these
1. **Two name spaces, two syntaxes, never merged** (SQL*Plus; JupySQL `:x` vs `{{x}}`; PL/SQL Dev's "Substitution" *type* in the same grid is a nice UI unification while keeping semantics apart).
2. **Undefined ⇒ leave the text untouched** for macros (psql, usql, DbGate-empty, HeidiSQL-empty). For binds make it a policy: error (SQL*Plus) / auto-declare (Golden) / prompt (Toad).
3. **Find binds in the splitter's token stream** (psql flex states; DbGate shares its splitter tokenizer; DBeaver token rule). Consume `::` and `:=` as tokens *before* the bind rule. Where the engine can tell you, **ask the engine**: rusqlite `parameter_name`, rust-oracle `bind_names()` after prepare (Oracle's own parser skips strings/comments).
4. **Statement-class gate**: no bind scan in `CREATE … TRIGGER/PROCEDURE/FUNCTION/PACKAGE/TYPE` bodies (SQL*Plus behaviour; DBeaver `sql.parameter.ddl.enabled=false`); and PL/SQL Developer's automatic *bind→text downgrade* for statements that cannot take binds (DDL, `ALTER SESSION`, PG utility/`DO`).
5. **Never rescan substituted text** (psql quoted forms; counter-examples usql defect 3, Beekeeper #581).
6. **Safe quoting forms + format specifiers**: psql `:'v'`/`:"v"`, Grafana `${v:csv|sqlstring|json|regex}`; Redash's rule that free text must not be raw-substituted in shared contexts.
7. **WbCall result model**: OUT scalars as one `PARAMETER|VALUE` grid; one result tab per ref cursor *named after the parameter*; typed prompt for IN `?`; metadata from the statement first, catalog second.
8. **Golden's cursor UX** (grid, not text) + SQL*Plus semantics (one-shot; mark consumed cursors) + "ResultSet #n" for implicit results.
9. **`\gset`/WbVarDef row→vars with an explicit NULL/0-row/N-row policy.**
10. **Layered scope** (WbJ profile < workspace < command line) and **cbq per-variable stacks** for nested scripts.
11. **Inspector**: WbVarList editable grid, PL/SQL Dev changed-value highlight + "disabled, not deleted", DBeaver substituted-SQL preview, SQuirreL "hide text ⇒ don't cache".
12. **Server-evaluated assignment** (Neo4j `=>`, sqlite `.parameter set`): `EXEC :d := SYSDATE` should round-trip and keep the server's type.

### 4.2 Avoid these
- "Parameters" that are regex text replacement with user-supplied quotes (DBeaver, DataGrip, HeidiSQL, Beekeeper, DbGate, SQLTools, TablePlus): injection, wrong plans, no OUT.
- Sending every bind as VARCHAR2 (SQL Developer) — different peeked type ⇒ different plan than the application.
- Replacing inside strings and comments (SQL*Plus `&`, sqlcmd `$(`, WbJ `$[`, DBeaver `${`).
- One global on-disk value registry keyed by name only (DBeaver `parameter-bindings.xml`) — values bleed between servers/environments.
- A dialog per statement when running a script (DBeaver #34992).
- Same-named macro silently hijacking a native bind (usql).
- Executing a reformatted statement (Beekeeper via sql-formatter).
- `@` as a client parameter prefix on SQL Server/MySQL (DBeaver abandoned it; HeidiSQL refuses).
- Merging Int and Float, or NULL and missing (Neo4j `:params`, Mongo/Cosmos/Cassandra semantics).

### 4.3 Recommended abstract model
```
Script text ─ split (dialect-aware tokens) ─► Item
   Engine.plan(Item, VarStore, DriverCaps) ─► Action
       LocalAssign | NeedInput(prompt spec) | Execute(Request) | Print | …
   Request { text, params:[Param{name, dir In|Out|InOut|Return, ty, value}],
             captures:[RowToVars{names, policy}], macro_map (offset map for error lines) }
   Driver.execute(Request) ─► ResultStream = Event*
       RowSet{label, source: Primary|OutCursor(name)|Implicit(n)|ResultSet(n), rows…}
     | OutValues[(name, Value)] | ReturnStatus(i64) | Affected(n) | Message(notice/print/dbms_output)
   Engine.absorb(OutValues / captured row) ─► VarStore
```
- **VarStore** (extend `vars.rs`): `Value` = Missing | Null | Bool | Int | Float | Decimal(str) | Str | Bytes | Date/Timestamp/Duration | Array | Doc(ordered map) | Ext{tag,payload} | Cursor(handle) | **Query(text)** (lazy cursor for DBs without ref cursors). Per-var flags: `declared`, `secret`, `origin` (script/prompt/out/capture/profile), `changed_in_last_run`, `consumed` (cursor).
- **Macro store** separate (`&`, `DEFINE`, `:setvar`, args `&1..`, system vars `_USER`, `_DATE`, `_ROW_COUNT`, `_SQLCODE`, `_ELAPSED_MS`, `_TAB_NAME`, `_FILE`).
- **Minimum driver capability interface** (port + registry, per docs/30):
  `marker_style` {Named(':'), At('@'), DollarN, Question, DollarName, EngineReported} · `bind_named` · `bind_positional` · `bind_array` · `ident_placeholder` · `out_values` {None, Protocol, TailRow, ResultRow} · `return_status` · `cursor_out` {None, Handle, NameNeedsTxn} · `multi_result` · `implicit_results` · `paging` {None, ServerCursor, OpaqueToken} · `server_vars` {None, Session, SessionListable} · `server_eval_param_expr` · `unset_distinct_from_null` · `bind_scope` {Anywhere, DmlOnly, WhereOnly} · `describe_params` (engine can name/type params after prepare).
  Adapter functions: `to_native(Value, hint)`, `from_native → Value`, `quote_value(Value)`, `quote_ident(&str)`, `format(Value, Fmt)` (text fallback, per language: SQL, JSON, Cypher, CQL, Redis-arg), `classify(stmt) → {Query, Dml, Block, Ddl/Utility, Call}`.
- **Fallback ladder** per statement: real bind → (statement class cannot take binds) typed, adapter-quoted text substitution with a visible notice → refuse (raw text of an unvalidated Str on a PROD connection needs confirmation).

### 4.4 Per-DBMS technique for the five Oracle workflows
| Workflow | Oracle | SQL Server (tiberius) | PostgreSQL | MySQL/MariaDB | SQLite | NoSQL |
|---|---|---|---|---|---|---|
| W1 declare / implicit | client only | client only; map type → T-SQL type (`Auto` → `sql_variant`/`nvarchar(max)`) | client only; keep type to emit `$1::type` casts | client only | client only | client only, JSON value |
| W2a `EXEC :V := 'lit'` | local, 0 round trips | same | same | same | same | same |
| W2b `EXEC :V := expr` | `BEGIN :V := expr; END;` InOut bind | `DECLARE @V T = @P1; SET @V = expr; SELECT @V AS [V]` (tail row) | `SELECT (expr) AS "V"` → capture | `SELECT (expr) AS V` → capture | same | Neo4j: `RETURN expr`; else n/a |
| W2c `EXEC SELECT a,b INTO :A,:B` | block with OUT binds | rewrite to `SELECT @A=a,@B=b …` inside `sp_executesql`, **plus row-count guard** (T-SQL takes the last row silently — add `@@ROWCOUNT` to the tail row and apply the Oracle-like policy client-side) | strip INTO, run, capture row (policy: 0 rows / >1 rows = error, NULL = Null) | strip INTO + capture (avoid server `INTO @v`: 0 rows leaves the old value) | strip INTO + capture | strip + capture |
| W3 `:V` in later SQL | name bind | `sp_executesql N'…', N'@V T', @V=@P1`; if first statement must lead the batch (CREATE PROC…), prepend `DECLARE` or downgrade to text | `:V` → `$n` (+ cast from declared type); utility/`DO` → quoted text | `:V` → `?` (repeat value per occurrence) | bind by engine-reported name; decide whether `:x`, `@x`, `$x` share one variable | native named/positional params; text+`format()` where the language has none |
| W4 ref cursor / multi results | `OracleType::RefCursor` out bind → `OutCursor(name)` tab; `implicit_result()` loop → `Implicit(n)` tabs; nested cursor cells | no client cursor binds → every result set = `ResultSet(n)` tab; a `VAR c REFCURSOR` passed to a proc is a warning + "result sets shown as tabs" | call in a txn, read cursor *names* (getString-style), `FETCH ALL IN "n"` per name → tabs, `CLOSE`; refuse/auto-wrap in autocommit | result set n ↔ tab n; binary-PS OUT row → OutValues; MariaDB SYS_REFCURSOR is server-internal | `VAR c REFCURSOR; EXEC :c := SELECT…` stored as `Query` and run on PRINT [I] | driver paging stream → tab |
| W5 `&` vs `:` | both supported | `&`/`$(v)` macros + `@`-native | psql-compatible `:'v'` must NOT be enabled by default (collides with binds) — offer `&v`/`${v:fmt}` | same | same | `${v:json}` etc. |

---

## 5. Open product decisions

| # | Decision | Options | Recommendation |
|---|---|---|---|
| 1 | Bind-variable scope | (a) tab [Golden, HeidiSQL] (b) session/connection (c) global [SQL*Plus, WbJ default] (d) layered | **Tab-owned store as default + optional "shared with connection" layer + read-only profile layer** (WbJ layering). Survives CONNECT (docs/08 §3) but cursors invalidate. Fits DR-34 (`Sess` holds cursors, tab holds values). |
| 2 | Persistence across restart | none [most] / with hot-exit tab state [HeidiSQL, DbGate] / explicit variable-set files [WbJ, `.tst`] | Persist with the tab's hot-exit data under `NSQL_HOME`, **never** `secret` vars or cursors; explicit export/import as a script of `VAR`/`DEFINE` lines. |
| 3 | Undeclared bind | error [SQL*Plus] / auto-declare Auto [Golden, current nsql] / prompt [Toad] | Setting `vars.undeclared = auto|prompt|error`, default **auto for assignment targets, prompt for reads of a never-assigned name** (a read of an unknown bind is usually a typo or a missing input). Strict mode = SQL*Plus. |
| 4 | Prompt UX | per statement / once per run | Collect all missing binds + macros for the whole run **once**, typed grid with preview, "Ignore for this run" (DBeaver), remember last values per tab. |
| 5 | Cursor auto-print | off (SQL*Plus default) / on | **On in GUI** (each out cursor = result tab named after the variable; implicit results "ResultSet #n"), CLI follows `SET AUTOPRINT`. Mark cursor consumed; re-PRINT says "re-execute". Cap with existing fetch-limit settings. |
| 6 | Name case | insensitive/upper [Oracle, sqlcmd, DBeaver unquoted] / sensitive [SSMS, psql] | Case-insensitive, display as first written; `:"Quoted"` sensitive. For SQLite decide whether `:x/@x/$x` alias one variable — recommend **yes** (prefix-agnostic). |
| 7 | Typing strictness | untyped text / declared + Auto inference / strict | Keep `Auto` but **always show the inferred type** and send typed binds; coercion error on declared types; add `DATE/TIMESTAMP/BOOLEAN` beyond SQL*Plus (its lack of DATE is a known pain); never silently VARCHAR2. |
| 8 | Secrets | none / flag | `ACCEPT … HIDE` and name heuristics (`*PASS*|*PWD*|*SECRET*|*TOKEN*`) set `secret`: masked in inspector/log/TxLog, not persisted, not echoed by VERIFY, excluded from "copy as script". |
| 9 | `:name` inside PL/SQL & triggers | scan everything / class gate | Class gate from the splitter (CREATE … bodies = no binds; anonymous blocks = binds); on Oracle cross-check with `bind_names()` after prepare; `:NEW/:OLD` never prompt. |
| 10 | `&` handling | SQL*Plus-exact / safer | Default `SET DEFINE ON` only for Oracle-dialect tabs/scripts; never in comments (already), **not inside string literals unless `define.in_strings`**, status-bar `&` toggle (PL/SQL Dev); `$(v)` for MSSQL tabs; offer `${v:fmt}` universal form. |
| 11 | Text fallback when binds are impossible | silent / notice / refuse | Notice in the log + adapter quoting; on PROD connections confirm when a free-text value is substituted raw. |
| 12 | SELECT…INTO row policy | Oracle (error on 0/>1) / psql / WbJ first-row | Oracle policy by default, setting `vars.into_policy`; NULL ⇒ Null (not unset). |
| 13 | Server-side mirroring | never / opt-in | Opt-in per variable ("publish to session"): MSSQL `sp_set_session_context`, PG `set_config('nsql.v')`, MySQL `SET @v` — for server code that must see the value. Warn that PG GUCs are transactional. |
| 14 | Inspector panel | command only / panel | Panel: name · kind (bind/macro/system) · type · value · origin · changed-marker; edit in place; NULL vs empty; LOB/JSON viewer; open cursor; "copy as script"; hover in editor shows value; undefined-variable lint. (Hover/lint/rename exist in no surveyed tool.) |
| 15 | Parameter-table execution | none / feature | Later: "run once per row" of a grid/CSV using array binds where `bind_array` (Oracle, MariaDB), loop otherwise. No surveyed client has it. |
| 16 | NoSQL value entry | type dropdown / literal inference / JSON | Literal inference with shown type, JSON superset for Doc/Array, optional server evaluation where `server_eval_param_expr`. |

---

## 6. Performance & safety notes
- **Round trips**: literal assignment = 0; `EXEC :v := expr` = 1; MSSQL tail-SELECT retrieval adds no round trip (same batch) but is lost if the batch aborts — wrap in TRY/CATCH or accept "values unchanged on error" (Oracle semantics are the same: failed block ⇒ binds unchanged). PG refcursor = 1 call + 1 FETCH per cursor + CLOSE, inside a transaction (interacts with docs/56 manual-commit lock guard: an auto-opened txn must be closed by the tool).
- **Plan fidelity is the DBA reason for real binds**: same text + same bind types ⇒ same `sql_id`/plan as the application. Oracle: bind peeking at hard parse, adaptive cursor sharing; "With bind variables in general, the EXPLAIN PLAN output might not represent the real execution plan" [V tgsql]; SQL Developer's VARCHAR2-everything is the cautionary tale. PG: custom plans for 5 executions then generic-vs-custom comparison, `plan_cache_mode`, `EXPLAIN (GENERIC_PLAN)` PG16. SQL Server: parameter sniffing, `sp_executesql` plan reuse [M]. Offer "Explain with binds".
- **Statement reuse**: keep the rewritten text stable (deterministic placeholder numbering, no value-dependent text) so server caches hit; text fallback defeats this — another reason to show a notice.
- **Large values**: CLOB/BLOB binds streamed from file (PL/SQL Dev Long Raw import/export); inspector shows length + viewer, not the value; cap persisted value size; OUT VARCHAR2 buffer 32767 [M]; SESSION_CONTEXT 8000 B/value; psql-style text vars cannot hold NUL.
- **Injection**: macros are injection by design (psql docs say so explicitly). Mitigations: binds by default, quoting forms/format specifiers, no rescan, type-gated substitution (Redash), PROD confirmation, no substitution inside strings/comments by default.
- **Secrets**: mask in inspector, logs (docs/48), TxLog (docs/44) and hot-exit; remember that server-side logs may still record binds (PG `log_parameter_max_length`, XE `rpc_completed`, `V$SQL_BIND_CAPTURE`).
- **Resource governance** (docs/39): open out-cursors hold server resources (Oracle open_cursors, MariaDB `max_open_cursors` 50) — cap count, close on tab close/re-execute/disconnect, list them in the inspector.

## 7. Gaps in nsql-script found while reading (for the implementer) [I]
- The statement-class gate **already exists** [V]: `engine.rs` `is_stored_code_ddl` sends `CREATE … PROCEDURE|FUNCTION|PACKAGE|TRIGGER|TYPE|LIBRARY` as raw text (test covers `:NEW/:OLD`). Remaining scanner risk in `bind.rs`: digits are accepted as positional binds, so a PG array slice `a[1:3]` yields a false bind `:3` (DBeaver excludes a preceding `[` for this reason, at the cost of #37736) → make numeric binds Oracle-only and/or skip inside `[...]` on PG. Also PG utility statements / `DO` blocks need the bind→text downgrade.
- PG/MySQL/SQLite `SELECT … INTO :X` is "(후속)" — implement as client-side capture (strip INTO), not server `INTO @v`.
- tiberius cannot return OUTPUT params via protocol → the tail-SELECT technique is mandatory; add `@@ROWCOUNT` guard for W2c.
- SQLite: prefer engine-reported parameter names over `extract_binds`.
- Value model lacks Missing/Array/Doc/Ext/Query variants needed for NoSQL adapters.

## 8. Unverified items (do not rely on without checking)
SQL*Plus bind survival across CONNECT (inferred) · Toad value memory and `:NEW` handling · SQLcl `REPEAT`/`SCRIPT` details · SESSION_CONTEXT rollback behaviour · `+PEEKED_BINDS` official wording · whether DBeaver QM logs bound values · MariaDB SYS_REFCURSOR protocol exposure (absence of evidence) · introduction versions marked [M] · pgJDBC `Types.REF_CURSOR` · Navicat `[$p]` (search summary only) · Studio 3T / TablePlus NoSQL / Hasura not researched · Golden 6–8 specifics (help text is 3.x era).
