use super::*;

fn col(name: &str, ty: &str, nullable: bool) -> Column {
    Column {
        name: name.into(),
        data_type: ty.into(),
        nullable,
        ..Column::default()
    }
}

fn base() -> Table {
    Table {
        schema: "APP".into(),
        name: "ITEM".into(),
        comment: String::new(),
        columns: vec![
            col("ID", "NUMBER(10)", false),
            col("NAME", "VARCHAR2(50)", true),
            col("QTY", "NUMBER", true),
        ],
        keys: vec![Key {
            name: "ITEM_PK".into(),
            kind: KeyKind::Primary,
            cols: vec!["ID".into()],
            ..Key::default()
        }],
        indexes: vec![Index {
            name: "ITEM_IX1".into(),
            unique: false,
            cols: vec!["NAME".into()],
            origin: None,
        }],
    }
}

fn sqls(d: Dialect, o: &Table, e: &Table) -> Vec<String> {
    plan(d, o, &diff(o, e))
        .stmts
        .into_iter()
        .map(|s| s.sql)
        .collect()
}

#[test]
fn no_change_no_statement() {
    let o = base();
    let e = o.editable();
    assert!(diff(&o, &e).is_empty());
    for d in [
        Dialect::Oracle,
        Dialect::Postgres,
        Dialect::Mssql,
        Dialect::Mysql,
        Dialect::Sqlite,
    ] {
        assert_eq!(plan(d, &o, &diff(&o, &e)), Plan::default());
    }
}

#[test]
fn add_column_per_dialect() {
    let o = base();
    let mut e = o.editable();
    e.columns.push(Column {
        name: "FLAG".into(),
        data_type: "CHAR(1)".into(),
        nullable: true,
        default: "'N'".into(),
        comment: "flag".into(),
        origin: None,
    });
    assert_eq!(
        sqls(Dialect::Oracle, &o, &e),
        [
            "ALTER TABLE APP.ITEM ADD (FLAG CHAR(1) DEFAULT 'N')",
            "COMMENT ON COLUMN APP.ITEM.FLAG IS 'flag'"
        ]
    );
    assert_eq!(
        sqls(Dialect::Mssql, &o, &e),
        [
            "ALTER TABLE APP.ITEM ADD FLAG CHAR(1) CONSTRAINT DF_ITEM_FLAG DEFAULT 'N' NULL",
            "EXEC sys.sp_addextendedproperty @name = N'MS_Description', @value = N'flag', @level0type = N'SCHEMA', @level0name = N'APP', @level1type = N'TABLE', @level1name = N'ITEM', @level2type = N'COLUMN', @level2name = N'FLAG'"
        ]
    );
    assert_eq!(
        sqls(Dialect::Mysql, &o, &e),
        ["ALTER TABLE APP.ITEM ADD COLUMN FLAG CHAR(1) NULL DEFAULT 'N' COMMENT 'flag'"]
    );
    let p = plan(Dialect::Sqlite, &o, &diff(&o, &e));
    assert_eq!(
        p.stmts[0].sql,
        "ALTER TABLE APP.ITEM ADD COLUMN FLAG CHAR(1) DEFAULT 'N'"
    );
    assert_eq!(
        p.blocked,
        [Blocked {
            what: "FLAG".into(),
            reason: Reason::NotSupported
        }]
    );
}

#[test]
fn rename_then_alter_uses_new_name_and_order() {
    let o = base();
    let mut e = o.editable();
    e.columns[1].name = "TITLE".into();
    e.columns[1].data_type = "VARCHAR2(20)".into();
    e.columns[1].nullable = false;
    e.indexes[0].cols = vec!["TITLE".into()]; // 컬럼 이름만 따라 바뀐 인덱스 = 변경 아님
    let ch = diff(&o, &e);
    assert!(!ch
        .iter()
        .any(|c| matches!(c, Change::DropIndex(_) | Change::AddIndex(_))));
    let p = plan(Dialect::Oracle, &o, &ch);
    let s: Vec<&str> = p.stmts.iter().map(|s| s.sql.as_str()).collect();
    assert_eq!(
        s,
        [
            "ALTER TABLE APP.ITEM RENAME COLUMN NAME TO TITLE",
            "ALTER TABLE APP.ITEM MODIFY (TITLE VARCHAR2(20))",
            "ALTER TABLE APP.ITEM MODIFY (TITLE NOT NULL)"
        ]
    );
    assert_eq!(p.stmts[1].risk, Risk::Danger); // 50 → 20 축소
    assert_eq!(p.risk(), Risk::Danger);
    assert_eq!(
        sqls(Dialect::Postgres, &o, &e),
        [
            "ALTER TABLE \"APP\".\"ITEM\" RENAME COLUMN \"NAME\" TO \"TITLE\"",
            "ALTER TABLE \"APP\".\"ITEM\" ALTER COLUMN \"TITLE\" TYPE VARCHAR2(20)",
            "ALTER TABLE \"APP\".\"ITEM\" ALTER COLUMN \"TITLE\" SET NOT NULL"
        ]
    );
    assert_eq!(
        sqls(Dialect::Mssql, &o, &e),
        [
            "EXEC sp_rename N'[APP].[ITEM].[NAME]', N'TITLE', N'COLUMN'",
            "ALTER TABLE APP.ITEM ALTER COLUMN TITLE VARCHAR2(20) NOT NULL"
        ]
    );
    assert_eq!(
        sqls(Dialect::Mysql, &o, &e),
        [
            "ALTER TABLE APP.ITEM RENAME COLUMN NAME TO TITLE",
            "ALTER TABLE APP.ITEM MODIFY COLUMN TITLE VARCHAR2(20) NOT NULL"
        ]
    );
    // 위험 검사는 적용 전 이름으로.
    let rc = risk_checks(Dialect::Oracle, &o, &ch);
    assert_eq!(rc.len(), 1);
    assert_eq!(rc[0].kind, CheckKind::NullRows);
    assert_eq!(
        rc[0].sql,
        "SELECT COUNT(*) FROM APP.ITEM WHERE NAME IS NULL"
    );
}

#[test]
fn drop_column_is_danger_and_checked() {
    let mut o = base();
    o.columns[2].default = "0".into();
    let mut e = o.editable();
    e.columns.remove(2);
    let ch = diff(&o, &e);
    let p = plan(Dialect::Mssql, &o, &ch);
    assert_eq!(p.stmts.len(), 2);
    assert!(p.stmts[0].sql.starts_with("DECLARE @nsql_df sysname"));
    assert!(p.stmts[0].sql.ends_with(
        "DROP CONSTRAINT ' + QUOTENAME(@nsql_df); IF @nsql_sql IS NOT NULL EXEC(@nsql_sql)"
    ));
    assert!(p.stmts[0]
        .sql
        .contains("OBJECT_ID(N'[APP].[ITEM]') AND c.name = N'QTY'"));
    assert_eq!(p.stmts[1].sql, "ALTER TABLE APP.ITEM DROP COLUMN QTY");
    assert_eq!(p.risk(), Risk::Danger);
    assert_eq!(
        sqls(Dialect::Sqlite, &o, &e),
        ["ALTER TABLE APP.ITEM DROP COLUMN QTY"]
    );
    assert_eq!(
        risk_checks(Dialect::Oracle, &o, &ch)[0].sql,
        "SELECT COUNT(*) FROM APP.ITEM WHERE QTY IS NOT NULL"
    );
}

#[test]
fn default_change_per_dialect() {
    let mut o = base();
    o.columns[2].default = "0".into();
    let mut e = o.editable();
    e.columns[2].default = "1".into();
    assert_eq!(
        sqls(Dialect::Oracle, &o, &e),
        ["ALTER TABLE APP.ITEM MODIFY (QTY DEFAULT 1)"]
    );
    let ms = sqls(Dialect::Mssql, &o, &e);
    assert_eq!(ms.len(), 2);
    assert_eq!(
        ms[1],
        "ALTER TABLE APP.ITEM ADD CONSTRAINT DF_ITEM_QTY DEFAULT 1 FOR QTY"
    );
    e.columns[2].default = String::new();
    assert_eq!(
        sqls(Dialect::Oracle, &o, &e),
        ["ALTER TABLE APP.ITEM MODIFY (QTY DEFAULT NULL)"]
    );
    assert_eq!(
        sqls(Dialect::Postgres, &o, &e),
        ["ALTER TABLE \"APP\".\"ITEM\" ALTER COLUMN \"QTY\" DROP DEFAULT"]
    );
    let p = plan(Dialect::Sqlite, &o, &diff(&o, &e));
    assert!(p.stmts.is_empty());
    assert_eq!(
        p.blocked,
        [Blocked {
            what: "QTY".into(),
            reason: Reason::NeedsRebuild
        }]
    );
}

#[test]
fn move_column_only_mysql() {
    let o = base();
    let mut e = o.editable();
    let q = e.columns.remove(2);
    e.columns.insert(0, q); // QTY를 맨 앞으로
    let ch = diff(&o, &e);
    assert_eq!(ch.len(), 1);
    assert!(matches!(&ch[0], Change::MoveColumn { col, after: None } if col.name == "QTY"));
    assert_eq!(
        sqls(Dialect::Mysql, &o, &e),
        ["ALTER TABLE APP.ITEM MODIFY COLUMN QTY NUMBER NULL FIRST"]
    );
    for d in [
        Dialect::Oracle,
        Dialect::Postgres,
        Dialect::Mssql,
        Dialect::Sqlite,
    ] {
        let p = plan(d, &o, &ch);
        assert!(p.stmts.is_empty(), "{d:?}");
        assert_eq!(
            p.blocked,
            [Blocked {
                what: "QTY".into(),
                reason: Reason::NeedsRebuild
            }]
        );
    }
}

#[test]
fn keys_and_indexes() {
    let o = base();
    let mut e = o.editable();
    e.keys.push(Key {
        name: "ITEM_FK1".into(),
        kind: KeyKind::Foreign,
        cols: vec!["QTY".into()],
        ref_table: "APP.UNIT".into(),
        ref_cols: vec!["ID".into()],
        on_delete: "cascade".into(),
        ..Key::default()
    });
    e.keys.push(Key {
        name: "ITEM_UK1".into(),
        kind: KeyKind::Unique,
        cols: vec!["NAME".into()],
        ..Key::default()
    });
    e.indexes[0].unique = true; // 바뀐 인덱스 = 지우고 다시
    e.indexes.push(Index {
        name: "ITEM_IX2".into(),
        unique: false,
        cols: vec!["QTY".into(), "NAME".into()],
        origin: None,
    });
    let ch = diff(&o, &e);
    assert_eq!(
        sqls(Dialect::Oracle, &o, &e),
        [
            "DROP INDEX APP.ITEM_IX1",
            "ALTER TABLE APP.ITEM ADD CONSTRAINT ITEM_UK1 UNIQUE (NAME)",
            "ALTER TABLE APP.ITEM ADD CONSTRAINT ITEM_FK1 FOREIGN KEY (QTY) REFERENCES APP.UNIT (ID) ON DELETE CASCADE",
            "CREATE UNIQUE INDEX APP.ITEM_IX1 ON APP.ITEM (NAME)",
            "CREATE INDEX APP.ITEM_IX2 ON APP.ITEM (QTY, NAME)"
        ]
    );
    let rc = risk_checks(Dialect::Oracle, &o, &ch);
    assert_eq!(rc.len(), 1);
    assert_eq!(rc[0].kind, CheckKind::DuplicateKeys);
    assert_eq!(rc[0].sql, "SELECT COUNT(*) FROM (SELECT NAME FROM APP.ITEM GROUP BY NAME HAVING COUNT(*) > 1) nsql_dup");
    // 갱신 규칙: 정의에 실리고(Oracle은 문법에 없어 뺀다) · 규칙만 달라져도 변경이다.
    let mut o4 = o.clone();
    o4.keys.push(Key {
        name: "ITEM_FK9".into(),
        kind: KeyKind::Foreign,
        cols: vec!["QTY".into()],
        ref_table: "UNIT".into(),
        ref_cols: vec!["ID".into()],
        on_update: "CASCADE".into(),
        ..Key::default()
    });
    let mut e4 = o4.editable();
    assert!(diff(&o4, &e4).is_empty());
    e4.keys[1].on_update = String::new();
    assert_eq!(
        sqls(Dialect::Postgres, &o4, &e4),
        [
            "ALTER TABLE \"APP\".\"ITEM\" DROP CONSTRAINT \"ITEM_FK9\"",
            "ALTER TABLE \"APP\".\"ITEM\" ADD CONSTRAINT \"ITEM_FK9\" FOREIGN KEY (\"QTY\") REFERENCES \"UNIT\" (\"ID\")"
        ]
    );
    e4.keys[1].on_update = "set null".into();
    assert!(sqls(Dialect::Mssql, &o4, &e4)[1].ends_with("REFERENCES UNIT (ID) ON UPDATE SET NULL"));
    assert!(sqls(Dialect::Oracle, &o4, &e4)[1].ends_with("REFERENCES UNIT (ID)"));
    // 제약 삭제: MySQL은 종류마다 다른 문장 · SQLite는 막힘.
    let mut e2 = o.editable();
    e2.keys.clear();
    assert_eq!(
        sqls(Dialect::Mysql, &o, &e2),
        ["ALTER TABLE APP.ITEM DROP PRIMARY KEY"]
    );
    assert_eq!(
        sqls(Dialect::Postgres, &o, &e2),
        ["ALTER TABLE \"APP\".\"ITEM\" DROP CONSTRAINT \"ITEM_PK\""]
    );
    assert_eq!(plan(Dialect::Sqlite, &o, &diff(&o, &e2)).blocked.len(), 1);
    // 이름만 바뀐 제약·인덱스.
    let mut e3 = o.editable();
    e3.keys[0].name = "PK_ITEM".into();
    e3.indexes[0].name = "IX_ITEM".into();
    assert_eq!(
        sqls(Dialect::Oracle, &o, &e3),
        [
            "ALTER TABLE APP.ITEM RENAME CONSTRAINT ITEM_PK TO PK_ITEM",
            "ALTER INDEX APP.ITEM_IX1 RENAME TO IX_ITEM"
        ]
    );
    assert_eq!(
        sqls(Dialect::Mssql, &o, &e3),
        [
            "EXEC sp_rename N'[APP].[ITEM_PK]', N'PK_ITEM', N'OBJECT'",
            "EXEC sp_rename N'[APP].[ITEM].[ITEM_IX1]', N'IX_ITEM', N'INDEX'"
        ]
    );
}

#[test]
fn comments_and_table_rename_last() {
    let mut o = base();
    o.columns[1].comment = "old".into();
    let mut e = o.editable();
    e.comment = "items".into();
    e.columns[1].comment = String::new();
    e.name = "ITEMS".into();
    assert_eq!(
        sqls(Dialect::Oracle, &o, &e),
        [
            "COMMENT ON COLUMN APP.ITEM.NAME IS ''",
            "COMMENT ON TABLE APP.ITEM IS 'items'",
            "ALTER TABLE APP.ITEM RENAME TO ITEMS"
        ]
    );
    let ms = sqls(Dialect::Mssql, &o, &e);
    assert!(ms[0]
        .starts_with("EXEC sys.sp_dropextendedproperty @name = N'MS_Description', @level0type"));
    assert!(ms[1].starts_with("EXEC sys.sp_addextendedproperty"));
    assert_eq!(ms[2], "EXEC sp_rename N'[APP].[ITEM]', N'ITEMS'");
    assert_eq!(
        sqls(Dialect::Mysql, &o, &e),
        [
            "ALTER TABLE APP.ITEM MODIFY COLUMN NAME VARCHAR2(50) NULL",
            "ALTER TABLE APP.ITEM COMMENT = 'items'",
            "RENAME TABLE APP.ITEM TO APP.ITEMS"
        ]
    );
}

#[test]
fn create_table_script() {
    let mut t = base();
    t.comment = "items".into();
    t.columns[0].comment = "key".into();
    let s = create_table(Dialect::Oracle, &t);
    assert_eq!(
        s[0].sql,
        "CREATE TABLE APP.ITEM (\n    ID NUMBER(10) NOT NULL,\n    NAME VARCHAR2(50),\n    QTY NUMBER,\n    CONSTRAINT ITEM_PK PRIMARY KEY (ID)\n)"
    );
    assert_eq!(s[1].sql, "CREATE INDEX APP.ITEM_IX1 ON APP.ITEM (NAME)");
    assert_eq!(s[2].sql, "COMMENT ON TABLE APP.ITEM IS 'items'");
    assert_eq!(s[3].sql, "COMMENT ON COLUMN APP.ITEM.ID IS 'key'");
    assert_eq!(
        script(Dialect::Oracle, &s[1..2]),
        "CREATE INDEX APP.ITEM_IX1 ON APP.ITEM (NAME);\n"
    );
    assert_eq!(
        script(Dialect::Mssql, &s[1..2]),
        "CREATE INDEX APP.ITEM_IX1 ON APP.ITEM (NAME)\nGO\n"
    );
    let my = create_table(Dialect::Mysql, &t);
    assert!(my[0].sql.ends_with(") COMMENT = 'items'"));
    assert!(my[0].sql.contains("ID NUMBER(10) NOT NULL COMMENT 'key'"));
}

#[test]
fn ident_quoting_and_fold() {
    assert_eq!(ident(Dialect::Oracle, "ITEM_CD"), "ITEM_CD");
    assert_eq!(ident(Dialect::Oracle, "item_cd"), "\"item_cd\"");
    assert_eq!(ident(Dialect::Oracle, "DATE"), "\"DATE\"");
    assert_eq!(ident(Dialect::Postgres, "item_cd"), "item_cd");
    assert_eq!(ident(Dialect::Postgres, "ItemCd"), "\"ItemCd\"");
    assert_eq!(ident(Dialect::Mssql, "Order Detail"), "[Order Detail]");
    assert_eq!(ident(Dialect::Mysql, "user"), "`user`");
    assert_eq!(ident(Dialect::Sqlite, "1st"), "\"1st\"");
    assert_eq!(fold_new_name(Dialect::Oracle, " new_col "), "NEW_COL");
    assert_eq!(fold_new_name(Dialect::Postgres, "NewCol"), "newcol");
    assert_eq!(fold_new_name(Dialect::Oracle, "\"mixedCase\""), "mixedCase");
    assert_eq!(fold_new_name(Dialect::Mssql, "NewCol"), "NewCol");
}

#[test]
fn type_risk_and_lis() {
    assert_eq!(
        type_change_risk("VARCHAR2(50)", "varchar2( 20 )"),
        Risk::Danger
    );
    assert_eq!(
        type_change_risk("VARCHAR2(50)", "VARCHAR2(100)"),
        Risk::Warn
    );
    assert_eq!(type_change_risk("NUMBER", "VARCHAR2(10)"), Risk::Warn);
    // 같은 타입의 다른 이름 = 변경 아님(서버 정규 표기 ↔ 사람이 친 줄임).
    assert_eq!(norm_type("character varying(20)"), norm_type("varchar(20)"));
    assert_eq!(
        norm_type("timestamp without time zone"),
        norm_type("TIMESTAMP")
    );
    assert_eq!(norm_type("int4"), norm_type("integer"));
    assert_ne!(norm_type("varchar(20)"), norm_type("varchar(30)"));
    assert_eq!(
        type_change_risk("character varying(50)", "varchar(20)"),
        Risk::Danger
    );
    assert_eq!(lis_keep(&[0, 1, 2]), [true, true, true]);
    assert_eq!(lis_keep(&[2, 0, 1]), [false, true, true]);
    assert_eq!(lis_keep(&[]), Vec::<bool>::new());
}

#[test]
fn add_not_null_without_default_is_warned() {
    let o = base();
    let mut e = o.editable();
    e.columns.insert(
        1,
        Column {
            name: "CODE".into(),
            data_type: "VARCHAR2(10)".into(),
            nullable: false,
            ..Column::default()
        },
    );
    let ch = diff(&o, &e);
    let p = plan(Dialect::Oracle, &o, &ch);
    assert_eq!(p.stmts[0].risk, Risk::Warn);
    // 가운데 넣기 = MySQL만 자리 지정 · 다른 방언은 끝에 더해짐을 알린다.
    assert_eq!(
        p.blocked,
        [Blocked {
            what: "CODE".into(),
            reason: Reason::NeedsRebuild
        }]
    );
    assert_eq!(
        sqls(Dialect::Mysql, &o, &e),
        ["ALTER TABLE APP.ITEM ADD COLUMN CODE VARCHAR2(10) NOT NULL AFTER ID"]
    );
    assert_eq!(
        risk_checks(Dialect::Oracle, &o, &ch)[0].kind,
        CheckKind::TableRows
    );
}
