# Keyboard shortcuts and the user keymap file (Keyboard Shortcuts)

**English** | [한국어](Keyboard-Shortcuts-ko)

Nexa SQL's keyboard shortcuts follow Sublime Text. There are two ways to change them.

| Way | Where | Features |
|---|---|---|
| Keyboard Shortcuts window | Menu **View ▸ Keyboard shortcuts** (or a shortcut entry in the Preferences window) | For each command: [Assign] → press the key combination (for a two-step chord, press the second combination within `ui.key_capture_chord_ms` = 1.2 s by default; otherwise the first combination alone is assigned · Esc cancels) · [Clear] · [Reset] · values are stored in the setting `key.<command id>` |
| User keymap file | `<settings folder>/Packages/User/Default.sublime-keymap` → `Default (Windows).sublime-keymap` / `Default (OSX).sublime-keymap` / `Default (Linux).sublime-keymap` | Sublime format as is · **conditions (context)** · two-step chords (`ctrl+k, ctrl+u`) · read only when the file exists |

- Settings folder = Windows `%APPDATA%\nexa-sql` · macOS `~/Library/Application Support/nexa-sql` · Linux XDG `~/.config/nexa-sql` · if the environment variable `NSQL_HOME` is set, it takes precedence.
- The files are re-read **at startup and whenever settings change**. The later file (per OS) overrides the earlier one (`Default.sublime-keymap`), and within one file **the later binding wins** (Sublime rule).
- If a file has an error, only that file is skipped and the app starts as usual (one `[keymap] <path>: …` line in the log).

- You can also assign a key in the Keyboard Shortcuts window to commands that have no default shortcut. Example = `view.minimap` (show/hide the minimap · 10-30), which you can also toggle with the Command Palette "View: Show Minimap" / "View: Hide Minimap".

## File format

```json
[
  { "keys": ["ctrl+alt+9"], "command": "file.save_as" },
  { "keys": ["ctrl+alt+9"], "command": "edit.upper_case",
    "context": [ { "key": "num_selections", "operand": 2 } ] },
  { "keys": ["ctrl+k", "ctrl+9"], "command": "file.new",
    "context": [ { "key": "selection_empty" } ] }
]
```

- `keys` = one key combination (single) or two (a two-step chord). `//` and `/* … */` comments and trailing commas are allowed.
- `command` = a command id of this app (the id in the Command Palette) or a common Sublime command name (`save` · `upper_case` · `duplicate_line` etc. → mapped automatically).
- Bindings without conditions are **added to the default keys** (the default keys keep working). If you take a combination that another command was using, it is removed from that command.
- In the Keyboard Shortcuts window, bindings from the file show **"(file)"** after the shortcut, and bindings with conditions appear as **"when …"** rows (read-only) at the end of the list.
- **The file takes precedence over the window** (D-397). If a combination you [Assign] in the window is used by the file for another command, that row shows the warning `Keymap file: {combination} = {command} (file wins)`; if you [Clear] a command whose key is set by the file, it shows `Keymap file binds {combination} (file wins; edit the file)`. To change it, edit the keymap file. Bindings with conditions are used only while their condition is true, so they are not subject to these notices.

## Condition (context) keys

`{ "key": …, "operator": …, "operand": …, "match_all": … }` · operators = `equal` (default) · `not_equal` · `regex_match` · `not_regex_match` · `regex_contains` · `not_regex_contains`. With several selections (cursors), the value is an array; with `match_all: true` all of them must match, otherwise **only the first selection** is checked (Sublime rule).

| Key | Value |
|---|---|
| `selection_empty` | Whether the selection is empty (true/false) |
| `has_selection` | Whether there is a selection |
| `num_selections` | Number of cursors/selections |
| `preceding_text` | Text before the cursor (same line) · per selection |
| `following_text` | Text after the cursor (same line) · per selection |
| `text` | Selected text · per selection |
| `read_only` | Whether the tab is read-only |
| `focus` · `panel` | Where the focus is now = `editor` · `grid` (result grid) · `explorer` · `find` · `search` · `extensions` · `project` · `bookmarks` · `details` · `outline` · `ai` |
| `panel_visible` · `panel_has_focus` | Whether the **Find/Replace panel** is visible · whether that panel has focus |
| `overlay_visible` | Whether the Command Palette window is open |
| `auto_complete_visible` | Whether the code completion popup is open |
| `connected` · `dialect` | Whether the **current session** is connected · display name of the server type (`Oracle` · `SQL Server` · `PostgreSQL` · `MySQL` · `SQLite` · `ODBC` · `DuckDB` · `ClickHouse` · `Db2` · `MS Access` — even before connecting, the session's default type is given) |
| `setting.<key>` | Setting value (e.g. `setting.ui.lang`) |
| `selector` | Syntax scope at the cursor — `source.sql` + string `string.quoted.sql` · comment `comment.sql` · reserved word `keyword.sql` · number `constant.numeric.sql` · data type `storage.type.sql` · function `support.function.sql` · bind variable `variable.parameter.sql` · delimiter `punctuation.terminator.sql` · command `keyword.control.sql` (tab without a highlighter = `text.plain`) |

`selector` uses a subset of Sublime selectors: `,` and `|` = alternatives · space = descendant (in order) · `-` = exclude · prefix match on dot boundaries (`string` matches `string.quoted.sql`). `equal`/`not_equal` = whether the selector matches · regular expression operators apply to the scope text. Example = only inside strings:

```json
{ "keys": ["ctrl+alt+q"], "command": "edit.upper_case",
  "context": [{ "key": "selector", "operator": "equal", "operand": "source.sql string" }] }
```

To check = the `key selector = …` line of the startup command `keymap.dump:`.
