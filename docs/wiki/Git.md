# Git in the editor

**English** | [한국어](Git-ko)

When a file you open is tracked by git, Nexa SQL compares it with the version in git **HEAD** and shows what changed. It only reads — it never commits, pushes or fetches. Git must be on PATH.

## Change marks and the status bar

- **Gutter band** (`editor.git_gutter` · on) — a second band right of the save marks: lines **added** (green) · **modified** (accent) · a red wedge where lines were **deleted above**. Click a mark to see that change next to the HEAD text in a small popup (with `editor.git_context` = 3 lines of context).
- **Status bar** (`statusbar.git` · on) — the branch and the number of changed files, for example `main (3)`. If the branch tracks a remote branch you have already fetched, it also shows how far you are ahead/behind: `main ↑2 ↓1 (3)` (a side that is 0 is left out). This uses only what is already on your PC — it never contacts the remote.
- Refresh = every `statusbar.git_secs` seconds (15) while idle, right after you save and when you switch tabs. The gutter keeps following the file even if you turn the status bar segment off.

## Commands

In the Command Palette (Ctrl+Shift+P) under **Edit** · you can give them a key in the Keyboard Shortcuts window:

| Command | id | What it does |
|---|---|---|
| Next git change / Previous git change | `git.next_hunk` · `git.prev_hunk` | Moves the cursor to the next/previous changed block (wraps around) · "No git changes in this tab" if there is none |
| Revert git change at cursor | `git.revert_hunk` | Puts the HEAD text back for the changed block under the cursor — **one Undo step**, so Ctrl+Z brings your edit back · the file is **not saved** until you save it |

Large files are compared line by line precisely: only the lines that really changed are marked, even in files of many thousands of lines.
