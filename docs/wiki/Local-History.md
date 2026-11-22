# Local history

**English** | [한국어](Local-History-ko)

Nexa SQL keeps a **snapshot** of your work on your own PC — every time you save a file, and every time you open or compile a database object (view · procedure · package · trigger …). If you overwrite something by mistake, open the list of snapshots, compare one with the current text, and put it back.

- Nothing is sent anywhere — snapshots are plain `.sql` files in the app data folder (`history/` next to `settings.conf`).
- On by default (`history.enabled`). Old snapshots are removed automatically (count and size limits below) · **pinned** snapshots are always kept.

## Commands

All four are in the **Command Palette** (Ctrl+Shift+P) under **File** · you can give them a key in the Keyboard Shortcuts window.

| Command (palette) | id | What it does |
|---|---|---|
| File: Local history… | `edit.history` | Lists the snapshots of the active tab (newest first) · pick one to **compare** it with the current text in a compare tab |
| File: Restore from local history… | `edit.history_restore` | Same list · pick one to put that snapshot's text **into the active tab** · nothing is sent to the server (for an object, press F5 yourself to compile it) · the restore itself is kept as a snapshot |
| File: Pin or unpin a local history snapshot… | `edit.history_pin` | Same list · pick one to pin it (📌 in the list · never removed by the limits) or to unpin it |
| File: Check the server for changes to this object | `edit.history_check` | On an object source tab: reads the object from the server again and compares it with the last server version you have · same = "server source unchanged since <time>" · different = saved as an **external** snapshot + status bar and log message "server source changed since <time>" |

When a server check (manual or automatic) finds a change, a **notification card** also appears — click it to switch to that object tab and compare the server version with the editor.

The commands work on the **active tab**: a file tab (snapshots of that file) or an object source tab opened from the explorer (snapshots of that object). The server check works only on an object source tab.

## Kinds of snapshot

| Kind (in the file name) | When | Content |
|---|---|---|
| `save` | You save a file | The file as saved |
| `open` | You open an object from the explorer | The source the server returned — your "original" |
| `compile` | Just before you compile (F5) an object | Your buffer, even if the server rejects it |
| `server` | After a successful compile | The source the server now holds (re-read) |
| `restore` | You restore a snapshot | The text you put back |
| `external` | A server check finds a change | The new server source (someone else changed it) |

The same content is never stored twice in a row.

## Settings

In the Settings window (search "Local history"):

| Key | Default | Meaning |
|---|---|---|
| `history.enabled` | on | Keep snapshots at all |
| `history.max_per_item` | 50 | Snapshots per file or object · the oldest beyond this are removed (pinned ones are kept) |
| `history.max_mb` | 200 | Total size of all snapshots · the oldest are removed first |
| `history.server_check_secs` | 0 (off) | Check the server automatically **when you switch to an object source tab**, if at least this many seconds passed since that tab's last check · a change is saved as an `external` snapshot and shown in the status bar · no change = nothing is shown · it never polls in the background and only uses a connection you already have |
