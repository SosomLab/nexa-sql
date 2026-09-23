//! **내장 함수 · 시스템 패키지 · 사전(카탈로그) 객체의 정적 표**(docs/29 §6-4 · docs/76 §7 · T-178 · 사용자 09-23 "인텔리센스 전반 재검토").
//!
//! 서버에 묻지 않고도 완성 후보가 되는 것들 — 방언 공통(ANSI) + 방언별. 각 항목은 이름과 **시그니처 한 줄**(팝업 오른쪽 열 ·
//! `(`를 친 뒤 상태줄 시그니처 도움). 표는 "자주 쓰는 것" 기준이며 빠진 함수는 문서 단어로도 보완된다(`intel.document_words`).
//! 카탈로그 객체(`ALL_TABLES` · `pg_catalog.pg_class` · `sys.tables` · `INFORMATION_SCHEMA.TABLES`)는 관계 자리(FROM 뒤)와
//! `qualifier.` 뒤(`sys.` · `INFORMATION_SCHEMA.` · `pg_catalog.`)의 후보다.

use nsql_core::Dialect;

/// 내장 함수(또는 패키지 멤버) 하나 — `sig`는 표시용 시그니처(`NVL(expr1, expr2)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Builtin {
    pub name: &'static str,
    pub sig: &'static str,
}

/// 시스템 패키지(Oracle `DBMS_*`) — `PKG.` 뒤 멤버 완성.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SysPackage {
    pub name: &'static str,
    pub members: &'static [Builtin],
}

macro_rules! b {
    ($n:literal, $s:literal) => {
        Builtin { name: $n, sig: $s }
    };
}

/// ANSI 공통(모든 방언).
pub const COMMON: &[Builtin] = &[
    b!("COUNT", "COUNT(* | [DISTINCT] expr)"),
    b!("SUM", "SUM([DISTINCT] expr)"),
    b!("AVG", "AVG([DISTINCT] expr)"),
    b!("MIN", "MIN(expr)"),
    b!("MAX", "MAX(expr)"),
    b!("COALESCE", "COALESCE(expr1, expr2, …)"),
    b!("NULLIF", "NULLIF(expr1, expr2)"),
    b!("CAST", "CAST(expr AS type)"),
    b!("ABS", "ABS(n)"),
    b!("ROUND", "ROUND(n [, places])"),
    b!("FLOOR", "FLOOR(n)"),
    b!("CEILING", "CEILING(n)"),
    b!("MOD", "MOD(n, m)"),
    b!("POWER", "POWER(n, p)"),
    b!("SQRT", "SQRT(n)"),
    b!("LOWER", "LOWER(str)"),
    b!("UPPER", "UPPER(str)"),
    b!("LENGTH", "LENGTH(str)"),
    b!("SUBSTRING", "SUBSTRING(str FROM start [FOR len])"),
    b!("TRIM", "TRIM([LEADING | TRAILING | BOTH] [chars FROM] str)"),
    b!("LTRIM", "LTRIM(str [, chars])"),
    b!("RTRIM", "RTRIM(str [, chars])"),
    b!("REPLACE", "REPLACE(str, from, to)"),
    b!("CONCAT", "CONCAT(str1, str2, …)"),
    b!("POSITION", "POSITION(sub IN str)"),
    b!("CHAR_LENGTH", "CHAR_LENGTH(str)"),
    b!("EXTRACT", "EXTRACT(field FROM datetime)"),
    b!("CURRENT_DATE", "CURRENT_DATE"),
    b!("CURRENT_TIMESTAMP", "CURRENT_TIMESTAMP"),
    b!("ROW_NUMBER", "ROW_NUMBER() OVER (…)"),
    b!("RANK", "RANK() OVER (…)"),
    b!("DENSE_RANK", "DENSE_RANK() OVER (…)"),
    b!("LAG", "LAG(expr [, offset [, default]]) OVER (…)"),
    b!("LEAD", "LEAD(expr [, offset [, default]]) OVER (…)"),
    b!("FIRST_VALUE", "FIRST_VALUE(expr) OVER (…)"),
    b!("LAST_VALUE", "LAST_VALUE(expr) OVER (…)"),
    b!("NTILE", "NTILE(n) OVER (…)"),
    b!("GREATEST", "GREATEST(expr1, expr2, …)"),
    b!("LEAST", "LEAST(expr1, expr2, …)"),
];

pub const ORACLE: &[Builtin] = &[
    b!("NVL", "NVL(expr1, expr2)"),
    b!("NVL2", "NVL2(expr, if_not_null, if_null)"),
    b!("DECODE", "DECODE(expr, search1, result1 [, …] [, default])"),
    b!("TO_CHAR", "TO_CHAR(value [, format [, nls]])"),
    b!("TO_DATE", "TO_DATE(str [, format [, nls]])"),
    b!("TO_NUMBER", "TO_NUMBER(str [, format])"),
    b!("TO_TIMESTAMP", "TO_TIMESTAMP(str [, format])"),
    b!("SYSDATE", "SYSDATE"),
    b!("SYSTIMESTAMP", "SYSTIMESTAMP"),
    b!("TRUNC", "TRUNC(n | date [, places | format])"),
    b!("ADD_MONTHS", "ADD_MONTHS(date, n)"),
    b!("MONTHS_BETWEEN", "MONTHS_BETWEEN(date1, date2)"),
    b!("LAST_DAY", "LAST_DAY(date)"),
    b!("NEXT_DAY", "NEXT_DAY(date, day)"),
    b!("SUBSTR", "SUBSTR(str, start [, len])"),
    b!("INSTR", "INSTR(str, sub [, start [, nth]])"),
    b!("LPAD", "LPAD(str, len [, pad])"),
    b!("RPAD", "RPAD(str, len [, pad])"),
    b!("INITCAP", "INITCAP(str)"),
    b!("REGEXP_LIKE", "REGEXP_LIKE(str, pattern [, flags])"),
    b!(
        "REGEXP_SUBSTR",
        "REGEXP_SUBSTR(str, pattern [, pos [, nth [, flags]]])"
    ),
    b!(
        "REGEXP_REPLACE",
        "REGEXP_REPLACE(str, pattern [, replace [, pos [, nth [, flags]]]])"
    ),
    b!(
        "REGEXP_INSTR",
        "REGEXP_INSTR(str, pattern [, pos [, nth [, opt [, flags]]]])"
    ),
    b!("LISTAGG", "LISTAGG(expr [, sep]) WITHIN GROUP (ORDER BY …)"),
    b!("WM_CONCAT", "WM_CONCAT(expr)"),
    b!("ROWNUM", "ROWNUM"),
    b!("ROWID", "ROWID"),
    b!("SYS_CONTEXT", "SYS_CONTEXT(namespace, parameter)"),
    b!("USERENV", "USERENV(parameter)"),
    b!("SYS_GUID", "SYS_GUID()"),
    b!("CHR", "CHR(n)"),
    b!("ASCII", "ASCII(char)"),
    b!("NLSSORT", "NLSSORT(str [, nls])"),
    b!("RAWTOHEX", "RAWTOHEX(raw)"),
    b!("HEXTORAW", "HEXTORAW(hex)"),
    b!("DBMS_LOB.GETLENGTH", "DBMS_LOB.GETLENGTH(lob)"),
    b!("XMLAGG", "XMLAGG(xmlelement)"),
    b!("XMLELEMENT", "XMLELEMENT(name, value)"),
    b!("JSON_VALUE", "JSON_VALUE(json, path [RETURNING type])"),
    b!("JSON_OBJECT", "JSON_OBJECT(key VALUE expr, …)"),
    b!(
        "RAISE_APPLICATION_ERROR",
        "RAISE_APPLICATION_ERROR(-20000..-20999, message)"
    ),
    b!("SQLERRM", "SQLERRM"),
    b!("SQLCODE", "SQLCODE"),
];

pub const POSTGRES: &[Builtin] = &[
    b!("NOW", "NOW()"),
    b!("AGE", "AGE(timestamp [, timestamp])"),
    b!("DATE_TRUNC", "DATE_TRUNC(field, source)"),
    b!("DATE_PART", "DATE_PART(field, source)"),
    b!("TO_CHAR", "TO_CHAR(value, format)"),
    b!("TO_DATE", "TO_DATE(str, format)"),
    b!("TO_TIMESTAMP", "TO_TIMESTAMP(str, format)"),
    b!("TO_NUMBER", "TO_NUMBER(str, format)"),
    b!("SUBSTR", "SUBSTR(str, start [, len])"),
    b!("STRPOS", "STRPOS(str, sub)"),
    b!("SPLIT_PART", "SPLIT_PART(str, delim, n)"),
    b!("STRING_AGG", "STRING_AGG(expr, sep [ORDER BY …])"),
    b!("ARRAY_AGG", "ARRAY_AGG(expr [ORDER BY …])"),
    b!("UNNEST", "UNNEST(array)"),
    b!("ARRAY_LENGTH", "ARRAY_LENGTH(array, dim)"),
    b!("GENERATE_SERIES", "GENERATE_SERIES(start, stop [, step])"),
    b!(
        "REGEXP_REPLACE",
        "REGEXP_REPLACE(str, pattern, replace [, flags])"
    ),
    b!("REGEXP_MATCHES", "REGEXP_MATCHES(str, pattern [, flags])"),
    b!("LPAD", "LPAD(str, len [, fill])"),
    b!("RPAD", "RPAD(str, len [, fill])"),
    b!("INITCAP", "INITCAP(str)"),
    b!("MD5", "MD5(str)"),
    b!("GEN_RANDOM_UUID", "GEN_RANDOM_UUID()"),
    b!("JSONB_BUILD_OBJECT", "JSONB_BUILD_OBJECT(key, value, …)"),
    b!("JSONB_AGG", "JSONB_AGG(expr)"),
    b!("JSON_AGG", "JSON_AGG(expr)"),
    b!("TO_JSON", "TO_JSON(anyelement)"),
    b!("PG_SLEEP", "PG_SLEEP(seconds)"),
    b!("PG_TYPEOF", "PG_TYPEOF(any)"),
    b!("CURRENT_USER", "CURRENT_USER"),
    b!("CURRENT_SCHEMA", "CURRENT_SCHEMA()"),
    b!("VERSION", "VERSION()"),
    b!("NEXTVAL", "NEXTVAL('sequence')"),
    b!("CURRVAL", "CURRVAL('sequence')"),
    b!("SETVAL", "SETVAL('sequence', value)"),
    b!("BOOL_AND", "BOOL_AND(expr)"),
    b!("BOOL_OR", "BOOL_OR(expr)"),
    b!(
        "PERCENTILE_CONT",
        "PERCENTILE_CONT(fraction) WITHIN GROUP (ORDER BY …)"
    ),
];

pub const MSSQL: &[Builtin] = &[
    b!("ISNULL", "ISNULL(check_expr, replacement)"),
    b!("IIF", "IIF(condition, true_value, false_value)"),
    b!("GETDATE", "GETDATE()"),
    b!("SYSDATETIME", "SYSDATETIME()"),
    b!("DATEADD", "DATEADD(datepart, number, date)"),
    b!("DATEDIFF", "DATEDIFF(datepart, start, end)"),
    b!("DATEPART", "DATEPART(datepart, date)"),
    b!("DATENAME", "DATENAME(datepart, date)"),
    b!("EOMONTH", "EOMONTH(date [, months])"),
    b!("FORMAT", "FORMAT(value, format [, culture])"),
    b!("CONVERT", "CONVERT(type, expr [, style])"),
    b!("TRY_CONVERT", "TRY_CONVERT(type, expr [, style])"),
    b!("TRY_CAST", "TRY_CAST(expr AS type)"),
    b!("LEN", "LEN(str)"),
    b!("DATALENGTH", "DATALENGTH(expr)"),
    b!("SUBSTRING", "SUBSTRING(str, start, len)"),
    b!("CHARINDEX", "CHARINDEX(sub, str [, start])"),
    b!("PATINDEX", "PATINDEX('%pattern%', str)"),
    b!("STUFF", "STUFF(str, start, len, replace)"),
    b!(
        "STRING_AGG",
        "STRING_AGG(expr, sep) [WITHIN GROUP (ORDER BY …)]"
    ),
    b!("STRING_SPLIT", "STRING_SPLIT(str, sep)"),
    b!("NEWID", "NEWID()"),
    b!("SCOPE_IDENTITY", "SCOPE_IDENTITY()"),
    b!("@@IDENTITY", "@@IDENTITY"),
    b!("@@ROWCOUNT", "@@ROWCOUNT"),
    b!("@@ERROR", "@@ERROR"),
    b!("@@VERSION", "@@VERSION"),
    b!("OBJECT_ID", "OBJECT_ID('name' [, type])"),
    b!("OBJECT_NAME", "OBJECT_NAME(id)"),
    b!("DB_NAME", "DB_NAME([id])"),
    b!("SUSER_SNAME", "SUSER_SNAME()"),
    b!("ERROR_MESSAGE", "ERROR_MESSAGE()"),
    b!("ERROR_NUMBER", "ERROR_NUMBER()"),
    b!("RAISERROR", "RAISERROR(message, severity, state)"),
    b!("THROW", "THROW [error, message, state]"),
    b!("CHECKSUM", "CHECKSUM(*| expr, …)"),
    b!("HASHBYTES", "HASHBYTES('SHA2_256', input)"),
    b!("JSON_VALUE", "JSON_VALUE(json, path)"),
    b!("JSON_QUERY", "JSON_QUERY(json [, path])"),
    b!("OPENJSON", "OPENJSON(json [, path]) [WITH (…)]"),
];

pub const MYSQL: &[Builtin] = &[
    b!("IFNULL", "IFNULL(expr1, expr2)"),
    b!("IF", "IF(condition, true_value, false_value)"),
    b!("NOW", "NOW()"),
    b!("CURDATE", "CURDATE()"),
    b!("DATE_FORMAT", "DATE_FORMAT(date, format)"),
    b!("STR_TO_DATE", "STR_TO_DATE(str, format)"),
    b!("DATE_ADD", "DATE_ADD(date, INTERVAL expr unit)"),
    b!("DATE_SUB", "DATE_SUB(date, INTERVAL expr unit)"),
    b!("DATEDIFF", "DATEDIFF(date1, date2)"),
    b!("TIMESTAMPDIFF", "TIMESTAMPDIFF(unit, start, end)"),
    b!("SUBSTR", "SUBSTR(str, pos [, len])"),
    b!("LOCATE", "LOCATE(sub, str [, pos])"),
    b!(
        "GROUP_CONCAT",
        "GROUP_CONCAT([DISTINCT] expr [ORDER BY …] [SEPARATOR sep])"
    ),
    b!("CONCAT_WS", "CONCAT_WS(sep, str1, str2, …)"),
    b!("LPAD", "LPAD(str, len, pad)"),
    b!("RPAD", "RPAD(str, len, pad)"),
    b!("LAST_INSERT_ID", "LAST_INSERT_ID()"),
    b!("UUID", "UUID()"),
    b!("MD5", "MD5(str)"),
    b!("SHA2", "SHA2(str, bits)"),
    b!("JSON_EXTRACT", "JSON_EXTRACT(json, path, …)"),
    b!("JSON_OBJECT", "JSON_OBJECT(key, value, …)"),
    b!("JSON_ARRAYAGG", "JSON_ARRAYAGG(expr)"),
    b!("FOUND_ROWS", "FOUND_ROWS()"),
    b!("ROW_COUNT", "ROW_COUNT()"),
    b!("DATABASE", "DATABASE()"),
    b!("VERSION", "VERSION()"),
];

pub const SQLITE: &[Builtin] = &[
    b!("IFNULL", "IFNULL(expr1, expr2)"),
    b!("IIF", "IIF(condition, true_value, false_value)"),
    b!("SUBSTR", "SUBSTR(str, start [, len])"),
    b!("INSTR", "INSTR(str, sub)"),
    b!("PRINTF", "PRINTF(format, …)"),
    b!("QUOTE", "QUOTE(value)"),
    b!("HEX", "HEX(blob)"),
    b!("RANDOM", "RANDOM()"),
    b!("RANDOMBLOB", "RANDOMBLOB(n)"),
    b!("TYPEOF", "TYPEOF(expr)"),
    b!("TOTAL", "TOTAL(expr)"),
    b!("GROUP_CONCAT", "GROUP_CONCAT(expr [, sep])"),
    b!("DATE", "DATE(time_value [, modifier, …])"),
    b!("TIME", "TIME(time_value [, modifier, …])"),
    b!("DATETIME", "DATETIME(time_value [, modifier, …])"),
    b!("JULIANDAY", "JULIANDAY(time_value [, modifier, …])"),
    b!("STRFTIME", "STRFTIME(format, time_value [, modifier, …])"),
    b!("UNIXEPOCH", "UNIXEPOCH(time_value [, modifier, …])"),
    b!("JSON_EXTRACT", "JSON_EXTRACT(json, path, …)"),
    b!("JSON_OBJECT", "JSON_OBJECT(label, value, …)"),
    b!("JSON_ARRAY", "JSON_ARRAY(value, …)"),
    b!("JSON_GROUP_ARRAY", "JSON_GROUP_ARRAY(value)"),
    b!("LAST_INSERT_ROWID", "LAST_INSERT_ROWID()"),
    b!("CHANGES", "CHANGES()"),
    b!("SQLITE_VERSION", "SQLITE_VERSION()"),
];

/// Oracle 시스템 패키지(`PKG.` 뒤 멤버).
pub const ORACLE_PACKAGES: &[SysPackage] = &[
    SysPackage {
        name: "DBMS_OUTPUT",
        members: &[
            b!("PUT_LINE", "DBMS_OUTPUT.PUT_LINE(item)"),
            b!("PUT", "DBMS_OUTPUT.PUT(item)"),
            b!("NEW_LINE", "DBMS_OUTPUT.NEW_LINE"),
            b!("ENABLE", "DBMS_OUTPUT.ENABLE([buffer_size])"),
            b!("DISABLE", "DBMS_OUTPUT.DISABLE"),
            b!("GET_LINE", "DBMS_OUTPUT.GET_LINE(line OUT, status OUT)"),
        ],
    },
    SysPackage {
        name: "DBMS_LOB",
        members: &[
            b!("GETLENGTH", "DBMS_LOB.GETLENGTH(lob)"),
            b!("SUBSTR", "DBMS_LOB.SUBSTR(lob, amount, offset)"),
            b!("INSTR", "DBMS_LOB.INSTR(lob, pattern [, offset [, nth]])"),
            b!("APPEND", "DBMS_LOB.APPEND(dest IN OUT, src)"),
            b!(
                "WRITEAPPEND",
                "DBMS_LOB.WRITEAPPEND(lob IN OUT, amount, buffer)"
            ),
            b!(
                "CREATETEMPORARY",
                "DBMS_LOB.CREATETEMPORARY(lob IN OUT, cache [, dur])"
            ),
            b!("FREETEMPORARY", "DBMS_LOB.FREETEMPORARY(lob IN OUT)"),
        ],
    },
    SysPackage {
        name: "DBMS_RANDOM",
        members: &[
            b!("VALUE", "DBMS_RANDOM.VALUE([low, high])"),
            b!("STRING", "DBMS_RANDOM.STRING(opt, len)"),
            b!("NORMAL", "DBMS_RANDOM.NORMAL"),
            b!("SEED", "DBMS_RANDOM.SEED(val)"),
        ],
    },
    SysPackage {
        name: "DBMS_UTILITY",
        members: &[
            b!(
                "FORMAT_ERROR_BACKTRACE",
                "DBMS_UTILITY.FORMAT_ERROR_BACKTRACE"
            ),
            b!("FORMAT_ERROR_STACK", "DBMS_UTILITY.FORMAT_ERROR_STACK"),
            b!("FORMAT_CALL_STACK", "DBMS_UTILITY.FORMAT_CALL_STACK"),
            b!("GET_TIME", "DBMS_UTILITY.GET_TIME"),
            b!(
                "COMMA_TO_TABLE",
                "DBMS_UTILITY.COMMA_TO_TABLE(list, tablen OUT, tab OUT)"
            ),
        ],
    },
    SysPackage {
        name: "DBMS_LOCK",
        members: &[b!("SLEEP", "DBMS_LOCK.SLEEP(seconds)")],
    },
    SysPackage {
        name: "DBMS_SESSION",
        members: &[
            b!("SET_IDENTIFIER", "DBMS_SESSION.SET_IDENTIFIER(client_id)"),
            b!(
                "SET_CONTEXT",
                "DBMS_SESSION.SET_CONTEXT(namespace, attribute, value)"
            ),
            b!(
                "CLEAR_CONTEXT",
                "DBMS_SESSION.CLEAR_CONTEXT(namespace [, client_id [, attribute]])"
            ),
            b!("SET_NLS", "DBMS_SESSION.SET_NLS(param, value)"),
        ],
    },
    SysPackage {
        name: "DBMS_APPLICATION_INFO",
        members: &[
            b!(
                "SET_MODULE",
                "DBMS_APPLICATION_INFO.SET_MODULE(module_name, action_name)"
            ),
            b!(
                "SET_ACTION",
                "DBMS_APPLICATION_INFO.SET_ACTION(action_name)"
            ),
            b!(
                "SET_CLIENT_INFO",
                "DBMS_APPLICATION_INFO.SET_CLIENT_INFO(client_info)"
            ),
        ],
    },
    SysPackage {
        name: "DBMS_SQL",
        members: &[
            b!("OPEN_CURSOR", "DBMS_SQL.OPEN_CURSOR RETURN INTEGER"),
            b!("PARSE", "DBMS_SQL.PARSE(c, statement, language_flag)"),
            b!("EXECUTE", "DBMS_SQL.EXECUTE(c) RETURN INTEGER"),
            b!("CLOSE_CURSOR", "DBMS_SQL.CLOSE_CURSOR(c IN OUT)"),
            b!(
                "TO_REFCURSOR",
                "DBMS_SQL.TO_REFCURSOR(cursor_number IN OUT) RETURN SYS_REFCURSOR"
            ),
        ],
    },
    SysPackage {
        name: "DBMS_STATS",
        members: &[
            b!(
                "GATHER_TABLE_STATS",
                "DBMS_STATS.GATHER_TABLE_STATS(ownname, tabname [, …])"
            ),
            b!(
                "GATHER_SCHEMA_STATS",
                "DBMS_STATS.GATHER_SCHEMA_STATS(ownname [, …])"
            ),
        ],
    },
    SysPackage {
        name: "DBMS_METADATA",
        members: &[b!(
            "GET_DDL",
            "DBMS_METADATA.GET_DDL(object_type, name [, schema])"
        )],
    },
    SysPackage {
        name: "DBMS_SCHEDULER",
        members: &[
            b!(
                "CREATE_JOB",
                "DBMS_SCHEDULER.CREATE_JOB(job_name, job_type, job_action [, …])"
            ),
            b!("RUN_JOB", "DBMS_SCHEDULER.RUN_JOB(job_name)"),
            b!("DROP_JOB", "DBMS_SCHEDULER.DROP_JOB(job_name)"),
        ],
    },
    SysPackage {
        name: "UTL_RAW",
        members: &[
            b!("CAST_TO_VARCHAR2", "UTL_RAW.CAST_TO_VARCHAR2(raw)"),
            b!("CAST_TO_RAW", "UTL_RAW.CAST_TO_RAW(str)"),
        ],
    },
];

/// 사전(카탈로그) 객체 — 관계 자리 후보 + `qualifier.` 뒤(`sys.` 등 점 앞 부분이 qualifier).
pub const ORACLE_OBJECTS: &[&str] = &[
    "DUAL",
    "ALL_TABLES",
    "ALL_TAB_COLUMNS",
    "ALL_OBJECTS",
    "ALL_VIEWS",
    "ALL_CONSTRAINTS",
    "ALL_CONS_COLUMNS",
    "ALL_INDEXES",
    "ALL_IND_COLUMNS",
    "ALL_SOURCE",
    "ALL_PROCEDURES",
    "ALL_ARGUMENTS",
    "ALL_SEQUENCES",
    "ALL_SYNONYMS",
    "ALL_TAB_COMMENTS",
    "ALL_COL_COMMENTS",
    "ALL_TRIGGERS",
    "ALL_USERS",
    "USER_TABLES",
    "USER_TAB_COLUMNS",
    "USER_OBJECTS",
    "USER_VIEWS",
    "USER_CONSTRAINTS",
    "USER_INDEXES",
    "USER_SOURCE",
    "USER_PROCEDURES",
    "USER_SEQUENCES",
    "USER_SYNONYMS",
    "USER_ERRORS",
    "USER_TRIGGERS",
    "DBA_TABLES",
    "DBA_OBJECTS",
    "DBA_USERS",
    "DBA_SEGMENTS",
    "DBA_DATA_FILES",
    "DBA_TABLESPACES",
    "V$SESSION",
    "V$SQL",
    "V$SQLAREA",
    "V$LOCK",
    "V$LOCKED_OBJECT",
    "V$PROCESS",
    "V$INSTANCE",
    "V$DATABASE",
    "V$PARAMETER",
    "V$VERSION",
    "GV$SESSION",
    "NLS_SESSION_PARAMETERS",
    "NLS_DATABASE_PARAMETERS",
];

pub const POSTGRES_OBJECTS: &[&str] = &[
    "pg_catalog.pg_class",
    "pg_catalog.pg_namespace",
    "pg_catalog.pg_attribute",
    "pg_catalog.pg_type",
    "pg_catalog.pg_proc",
    "pg_catalog.pg_index",
    "pg_catalog.pg_constraint",
    "pg_catalog.pg_tables",
    "pg_catalog.pg_views",
    "pg_catalog.pg_indexes",
    "pg_catalog.pg_roles",
    "pg_catalog.pg_database",
    "pg_catalog.pg_settings",
    "pg_catalog.pg_stat_activity",
    "pg_catalog.pg_locks",
    "pg_catalog.pg_stat_user_tables",
    "pg_catalog.pg_stat_statements",
    "pg_catalog.pg_description",
    "information_schema.tables",
    "information_schema.columns",
    "information_schema.views",
    "information_schema.routines",
    "information_schema.table_constraints",
    "information_schema.key_column_usage",
    "information_schema.schemata",
    "information_schema.sequences",
];

pub const MSSQL_OBJECTS: &[&str] = &[
    "sys.tables",
    "sys.columns",
    "sys.objects",
    "sys.views",
    "sys.schemas",
    "sys.indexes",
    "sys.index_columns",
    "sys.procedures",
    "sys.sql_modules",
    "sys.foreign_keys",
    "sys.key_constraints",
    "sys.types",
    "sys.databases",
    "sys.partitions",
    "sys.dm_exec_requests",
    "sys.dm_exec_sessions",
    "sys.dm_exec_sql_text",
    "sys.dm_tran_locks",
    "sys.dm_os_wait_stats",
    "INFORMATION_SCHEMA.TABLES",
    "INFORMATION_SCHEMA.COLUMNS",
    "INFORMATION_SCHEMA.VIEWS",
    "INFORMATION_SCHEMA.ROUTINES",
    "INFORMATION_SCHEMA.TABLE_CONSTRAINTS",
    "INFORMATION_SCHEMA.KEY_COLUMN_USAGE",
    "INFORMATION_SCHEMA.SCHEMATA",
];

pub const MYSQL_OBJECTS: &[&str] = &[
    "information_schema.TABLES",
    "information_schema.COLUMNS",
    "information_schema.VIEWS",
    "information_schema.ROUTINES",
    "information_schema.TABLE_CONSTRAINTS",
    "information_schema.KEY_COLUMN_USAGE",
    "information_schema.SCHEMATA",
    "information_schema.STATISTICS",
    "information_schema.PROCESSLIST",
    "performance_schema.threads",
    "mysql.user",
];

pub const SQLITE_OBJECTS: &[&str] = &[
    "sqlite_master",
    "sqlite_schema",
    "sqlite_sequence",
    "sqlite_temp_master",
    "pragma_table_info",
    "pragma_index_list",
    "pragma_foreign_key_list",
];

/// 방언별 내장 함수(공통 + 방언) — 이름은 대문자(`@@ROWCOUNT` 같은 T-SQL 전역 변수 포함).
#[must_use]
pub fn functions(d: Option<Dialect>) -> Vec<&'static Builtin> {
    let extra: &[Builtin] = match d {
        Some(Dialect::Oracle) => ORACLE,
        Some(Dialect::Postgres) => POSTGRES,
        Some(Dialect::Mssql) => MSSQL,
        Some(Dialect::Mysql) => MYSQL,
        Some(Dialect::Sqlite) => SQLITE,
        Some(Dialect::Odbc) | None => &[],
    };
    let mut out: Vec<&'static Builtin> = COMMON.iter().collect();
    for b in extra {
        // 방언 항목이 공통과 같은 이름이면 방언 시그니처가 이긴다(`SUBSTRING` MSSQL).
        if let Some(p) = out.iter().position(|c| c.name.eq_ignore_ascii_case(b.name)) {
            out[p] = b;
        } else {
            out.push(b);
        }
    }
    out
}

/// 시스템 패키지(지금은 Oracle만).
#[must_use]
pub fn packages(d: Option<Dialect>) -> &'static [SysPackage] {
    match d {
        Some(Dialect::Oracle) => ORACLE_PACKAGES,
        _ => &[],
    }
}

/// 이름으로 패키지 찾기(대소문자 무시).
#[must_use]
pub fn package(d: Option<Dialect>, name: &str) -> Option<&'static SysPackage> {
    packages(d)
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

/// 사전(카탈로그) 객체 이름들.
#[must_use]
pub fn system_objects(d: Option<Dialect>) -> &'static [&'static str] {
    match d {
        Some(Dialect::Oracle) => ORACLE_OBJECTS,
        Some(Dialect::Postgres) => POSTGRES_OBJECTS,
        Some(Dialect::Mssql) => MSSQL_OBJECTS,
        Some(Dialect::Mysql) => MYSQL_OBJECTS,
        Some(Dialect::Sqlite) => SQLITE_OBJECTS,
        Some(Dialect::Odbc) | None => &[],
    }
}

/// `qualifier.` 뒤의 사전 객체(점 앞이 qualifier와 같은 것 · 점 뒤 이름만).
#[must_use]
pub fn system_members(d: Option<Dialect>, qualifier: &str) -> Vec<&'static str> {
    system_objects(d)
        .iter()
        .filter_map(|o| {
            let (q, n) = o.rsplit_once('.')?;
            q.eq_ignore_ascii_case(qualifier).then_some(n)
        })
        .collect()
}

/// 시그니처 조회 — 함수 이름 또는 `PKG.MEMBER`(대소문자 무시).
#[must_use]
pub fn signature(d: Option<Dialect>, name: &str) -> Option<&'static str> {
    if let Some((pkg, member)) = name.rsplit_once('.') {
        if let Some(p) = package(d, pkg) {
            return p
                .members
                .iter()
                .find(|m| m.name.eq_ignore_ascii_case(member))
                .map(|m| m.sig);
        }
    }
    functions(d)
        .into_iter()
        .find(|b| b.name.eq_ignore_ascii_case(name))
        .map(|b| b.sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_functions_override_common_and_signatures_resolve() {
        let ora = functions(Some(Dialect::Oracle));
        assert!(ora.iter().any(|b| b.name == "NVL"));
        assert!(ora.iter().any(|b| b.name == "COALESCE"), "공통 포함");
        let ms = functions(Some(Dialect::Mssql));
        let sub = ms
            .iter()
            .find(|b| b.name == "SUBSTRING")
            .expect("substring");
        assert_eq!(
            sub.sig, "SUBSTRING(str, start, len)",
            "방언 시그니처가 이긴다"
        );
        assert_eq!(
            ms.iter().filter(|b| b.name == "SUBSTRING").count(),
            1,
            "중복 없음"
        );
        assert_eq!(
            signature(Some(Dialect::Oracle), "nvl"),
            Some("NVL(expr1, expr2)")
        );
        assert_eq!(
            signature(Some(Dialect::Oracle), "dbms_output.put_line"),
            Some("DBMS_OUTPUT.PUT_LINE(item)")
        );
        assert_eq!(
            signature(Some(Dialect::Sqlite), "dbms_output.put_line"),
            None
        );
        assert_eq!(signature(None, "COUNT"), Some("COUNT(* | [DISTINCT] expr)"));
        assert!(signature(Some(Dialect::Postgres), "no_such_fn").is_none());
    }

    #[test]
    fn packages_and_system_members() {
        assert!(package(Some(Dialect::Oracle), "DBMS_OUTPUT").is_some());
        assert!(package(Some(Dialect::Postgres), "DBMS_OUTPUT").is_none());
        let ms = system_members(Some(Dialect::Mssql), "sys");
        assert!(ms.contains(&"tables") && ms.contains(&"dm_exec_requests"));
        let is = system_members(Some(Dialect::Mssql), "information_schema");
        assert!(is.contains(&"TABLES"), "대소문자 무시");
        assert!(system_members(Some(Dialect::Oracle), "sys").is_empty());
        assert!(system_objects(Some(Dialect::Oracle)).contains(&"V$SESSION"));
        assert!(system_objects(None).is_empty());
    }

    /// 표의 모든 이름은 식별자 모양이고 시그니처는 비어 있지 않다(오타 방지).
    #[test]
    fn tables_are_well_formed() {
        for d in [
            None,
            Some(Dialect::Oracle),
            Some(Dialect::Postgres),
            Some(Dialect::Mssql),
            Some(Dialect::Mysql),
            Some(Dialect::Sqlite),
        ] {
            let fs = functions(d);
            let mut seen: Vec<String> = Vec::new();
            for b in fs {
                assert!(!b.name.is_empty() && !b.sig.is_empty(), "{b:?}");
                assert!(
                    b.name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '@' | '.')),
                    "{}",
                    b.name
                );
                let low = b.name.to_lowercase();
                assert!(!seen.contains(&low), "중복 {}", b.name);
                seen.push(low);
            }
            for p in packages(d) {
                assert!(!p.members.is_empty());
                for m in p.members {
                    assert!(m.sig.starts_with(p.name), "{}.{}", p.name, m.name);
                }
            }
        }
    }
}
