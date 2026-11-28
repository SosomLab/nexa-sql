# Table editor — change a table's structure in a grid

**English** | [한국어](Table-Editor-ko)

The table editor opens a table as a tab where you edit its **columns, keys/constraints and indexes** in a grid. While you edit, the generated DDL is shown live at the bottom. Nothing is sent to the server until you press **Apply…** and confirm the script in the preview window.

Works with Oracle · PostgreSQL · SQL Server · MySQL/MariaDB · SQLite. For other databases the status bar says "The table editor does not support this database".

![Table editor](images/table-editor.png)

*Table editor — the new column is marked `+`, the changed one `*`; the DDL preview follows every edit*

## Opening it

| Way | How |
|---|---|
| An existing table | Explorer: right-click a table ▸ **Edit Structure…** |
| A new table | Explorer: right-click the **Tables** folder ▸ **New Table…** |
| Command Palette | "Edit Structure" (`obj.edit_structure` · no default key — bind one in the keymap if you use it often) |

Each table opens in its own tab titled **Table: name** (a new one = **New table**). The tab shows a dot while it has changes that are not applied yet.

### Without a license

The table editor is a licensed feature. Without a license it still works (free for non-commercial use): the first time you open it after starting Nexa SQL, the tab shows a notice — "Table editor - licensed feature" — with **OK** and **License…**. **OK** becomes available after 3 seconds; until you press it the editor does not accept edits. The notice is shown once per run. **License…** opens the License window, where you can request a license for commercial use.

## The screen

- **Top** — table name and table comment.
- **Section buttons** — **Columns** · **Keys** · **Indexes** · **History**, then **Add** · **Remove** · **Up** · **Down**.
- **Right side** — "N statement(s) to apply" (or "No changes") and **Refresh** · **Revert** · **Apply…**.
- **Grid** — one row per column / constraint / index. The marker in front of a row is `+` for a new row and `*` for a changed one.
- **Warning line** — changes that cannot be applied (see *Limits*).
- **DDL preview** — the statements that Apply would run, updated on every edit. Read-only; right-click to copy.

| Section | Cells |
|---|---|
| Columns | Name · Type · Not null (check) · Default · Comment |
| Keys | Name · Kind (Primary / Unique / Foreign / Check) · Columns · References / expression |
| Indexes | Name · Unique (check) · Columns |

## Editing

- Click a selected cell again, double-click it, press **Enter**, or just start typing to edit. **Tab** moves to the next cell, **Esc** cancels.
- **Type list** — while you edit a **Type** cell in the Columns section, a list of that database's common types opens under the cell (above it when there is no room below). It narrows to the types that start with what you typed. **↑ / ↓** then **Enter**, or a click, puts the type in and finishes the edit; **Esc** closes only the list. You can still type any other type by hand.
- Click a check cell to flip it. For a **new** constraint, clicking Kind cycles Primary → Unique → Foreign → Check.
- **Add** puts a new row at the end. Set `tbledit.add_row` to **After the selected row** to put it under the selected row instead — only MySQL can really place a column in the middle; on the other databases the column is still added at the end of the table. **Delete** (or **Remove**) removes the selected row.
- **Revert** drops all edits and goes back to what was read from the server. **Refresh** reads the structure again.
- **Undo / redo** — **Ctrl+Z** takes back one change at a time: a confirmed cell, an added or removed row, a flipped check, a moved row, or a **Revert**. **Ctrl+Y** (or **Ctrl+Shift+Z**) puts it back (macOS: **Cmd+Z** / **Cmd+Shift+Z**). These are the usual `edit.undo` / `edit.redo` keys; up to 200 steps are kept. While a cell is open for editing, the same keys undo the typing inside the cell. Reading the structure from the server again (Refresh, or after an Apply) clears the steps.
- **Closing a tab with unapplied changes** — the first close keeps the tab and the status bar says "Unapplied table changes - close the tab again to discard them". Close it again within a few seconds (`editor.close_confirm_ms`, 3 seconds by default) to discard the edits. Nothing was sent to the server.
- New names follow the database's own folding — unquoted names become upper case on Oracle and lower case on PostgreSQL. Wrap a name in `"…"` to keep it exactly as typed. Turn `tbledit.fold_names` off to keep every new name as typed (it is quoted in the script when the database needs that).
- Type several columns as a comma-separated list in the Columns cell of a key or an index.
- **Foreign key definition** — the References cell is `table(column, …)` followed by the optional rules, for example `parent(id) ON DELETE SET NULL ON UPDATE CASCADE`. Leave a rule out for the database default (`NO ACTION`). A table in another schema is written `schema.table`. Oracle has no `ON UPDATE`. For a check, the cell is the expression.

## Applying

1. Press **Apply…**. Nexa SQL reads the table from the server once more and runs the risk checks (row counts for changes such as adding NOT NULL). With `tbledit.risk_check` off the counts are skipped — except on production connections, where they always run.
2. If the table was changed on the server after you opened it, **nothing is applied** — press Refresh and edit again.
3. The preview window shows the script with a header: the number of statements, a `PRODUCTION` line for production connections, `DANGER` / `WARNING` lines, a `NOTE` line when a changed constraint is dropped and added again, and any check result that is not zero. You can edit the script in this window (see *Editing the script in the preview* below).
4. Press **Apply** in that window. The statements run **one at a time** on that server's session. The table is backed up first (see [Explorers and filters](Explorers-and-Filters) — object backup).
5. If a statement fails, the run **stops there**: "Stopped after n of m statement(s)". The statements before it stay applied (DDL is not rolled back automatically), and the edits that were not applied stay in the tab so you can fix them and apply again. Set `tbledit.on_failure` to **Discard** to reload the structure from the server and drop those edits instead.
   - A changed constraint is applied as `DROP` then `ADD`. If the `ADD` fails on existing data, the run stops with the constraint **removed**: fix the data or the definition and apply again — only the `ADD` is left to run. The object backup has the original definition.
6. After a successful run the structure is read again and the tab is clean.

On a production connection the usual production guards apply (confirmation and backup). Set `tbledit.prod` to **Deny (read-only)** to open tables on production connections for viewing only.

### Editing the script in the preview

The preview window is editable. If you change the script, **Apply** runs **what the window shows**, not the generated statements:

- The text is split into statements — on SQL Server into batches at `GO` lines — and they run one at a time, with the same backup and the same stop-at-the-first-error rule. Parts that are only comments are skipped.
- Nexa SQL does not check an edited script again. It runs what you wrote, as if you had run it in an editor.
- Afterwards the structure is read again from the server. When every statement succeeded, the tab shows the table **as it now is** and the edits you had made in the grid are **dropped** — once you edit the script, the script is what counts, not the grid. A change you left out of the script has to be entered again.
- If the script stops at an error, it is handled like any stopped Apply: the grid edits stay (or are dropped when `tbledit.on_failure` is **Discard**).
- Whatever the script added on the server (a column, a key, an index) appears in the grid as part of the table.

## History

Every Apply is recorded, whether it finished or stopped. The **History** section lists the runs made in this tab, newest first:

| Column | Content |
|---|---|
| When | time of the run |
| Result | "Applied n of m statement(s)" or "Stopped after n of m" |
| Script file | the saved script |

Double-click a row (or press **Enter**) to open that script in an editor tab.

The statements that were sent are saved next to the table's object backups, in the `backups/objects` folder of the settings folder, as `<profile>_<schema>.<table>_TABLE.<timestamp>_applied.sql`. The header comment says how many statements were applied. For a stopped run it also has a `STOPPED` line, and the last statement in the file is the one that failed. A script that you edited in the preview has a `Source: script edited in the preview window` line.

Closing the tab clears the list. The files stay.

## Limits

- **No table re-creation.** A change that would need the table to be dropped and re-created is never applied — it is listed on the warning line instead.
  - Moving a column to another position: only MySQL/MariaDB can do it with a statement.
  - SQLite: changing a column's type, NOT NULL, default or constraints is blocked (adding and renaming columns works). SQLite has no comments.
- **Foreign keys and checks read from the server** — on Oracle · PostgreSQL · SQL Server · MySQL you can edit their columns and definition; the change is applied as `DROP` then `ADD` (see *Applying*). A constraint with an attribute the editor cannot write back is **locked** — its Columns and definition cells are dimmed, and it can only be renamed or removed: deferrable · disabled or not validated (`DISABLE` · `NOVALIDATE` · `NOT VALID` · `NOCHECK` · `NOT ENFORCED`) · PostgreSQL `MATCH FULL` · SQL Server `NOT FOR REPLICATION`. When you select a locked constraint, a one-line note under the grid says that it can only be renamed or removed. To change a locked constraint, remove it and create it again yourself in an editor. On other databases, or when the attributes could not be read, every foreign key and check read from the server is rename / remove only. New foreign keys and checks are free.
- An index that only backs a primary key or a unique constraint is listed under Keys, not under Indexes.
- Not in this version: views · sequences · partitions · storage options.

## Setting

All of these are in **Settings → Explorer**. The defaults are the behaviour described above.

| Key | Default | Meaning |
|---|---|---|
| `tbledit.enabled` | On | **Off** = the Edit Structure and New Table commands do nothing; the status bar says "The table editor is turned off (tbledit.enabled)" |
| `tbledit.fold_names` | On | **On** = new names follow the database's default case (Oracle upper, PostgreSQL lower; wrap a name in double quotes to keep it as typed) · **Off** = always as typed |
| `tbledit.add_row` | At the end | Where **Add** puts the new row: **At the end** · **After the selected row** (only MySQL can place a column in the middle) |
| `tbledit.on_failure` | Keep | After an apply that stopped: **Keep** = the edits that were not applied stay in the tab · **Discard** = the structure is read from the server again and those edits are dropped |
| `tbledit.risk_check` | On | Counts NULL rows, values in dropped columns and duplicate keys before the preview. **Off** = no counts. Always on for production connections |
| `tbledit.prod` | Allow | **Allow** = edit production connections with the production guards (confirmation, backup) · **Deny (read-only)** = structure editing is turned off on production connections |

Not settings (always on): the table is read from the server again right before applying, statements run one at a time and stop at the first error, tables are never dropped and re-created, and the risk checks always run on production connections.
