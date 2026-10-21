# Code editor

The editor is built for short CadQuery scripts: Python highlighting, smart
indentation, completion, search and breakpoints. It also takes care that
your work is never lost.

## Editing

| Feature | How |
| --- | --- |
| Syntax highlighting | Automatic. Colors follow the light or dark theme |
| Line numbers, current line | Always shown. The current-line highlight can be turned off |
| Vertical ruler | At column 79 by default (*Maximum line length*, 0 hides it) |
| Indent / unindent | Tab / Shift+Tab. With several lines selected, Tab indents them all. Indentation is 4 spaces |
| Auto-indent | Enter keeps the indentation, indents after a line ending with `:` and dedents after `return`, `pass`, `break`, `continue` or `raise` |
| Toggle comment | Ctrl+/ comments or uncomments the selected lines |
| Line wrap | Off by default (*Preferences > Editor*) |
| Font size | 13 by default (8–28) |

## Completion and call signatures

Press **Ctrl+Space** (or **Alt+/**) to complete the name under the cursor:

* a popup lists matching names (up to 200) and filters as you keep typing;
* **↑/↓** choose, **Enter** or **Tab** accept, **Esc** closes;
* right after `(` or `,`, the call signature of the function is shown
  instead, for example the parameters of `Workplane.box`.

Completion runs in the Python worker with [jedi](https://github.com/davidhalter/jedi).
It knows the script API (`show_object`, `cq`, …) and anything importable from
the selected environment. Install `jedi` in that environment to enable it.
Without jedi, the command does nothing.

## Search

**Ctrl+F** opens the search bar above the editor:

* matches are highlighted as you type, and the bar shows *N of M* (or *No
  matches*);
* **Enter** / **Shift+Enter** go to the next / previous match. From the
  editor, use **F3** / **Alt+F3**;
* **Aa** toggles *Match case*;
* **Esc** closes the bar.

## Breakpoints and markers

Click a line number to toggle a **breakpoint** (red dot). The gutter and line
backgrounds also show:

* the **current debug line** (yellow) while a debugging session is paused;
* the **error line** (red) after a failed run.

See [Debugging and inspection](debugging.md).

## Files

| Command | Shortcut | Notes |
| --- | --- | --- |
| New | Ctrl+N | Empty untitled script |
| Open… | Ctrl+O | Python files (`*.py`) |
| Open Recent | | The last 10 files |
| Save | Ctrl+S | Untitled scripts and examples ask for a location |
| Save As… | Ctrl+Shift+S | |
| Quit | Ctrl+Q | |

* CQ-editor reopens the last file when it starts. A file given on the
  command line takes precedence.
* Closing the window, opening another file or quitting with unsaved changes
  asks **Save**, **Don't save** or **Cancel**.
* **Examples** (File > Examples) open as read-only buffers. You can edit and
  run them; saving asks for a new location.

### Encoding and line endings

Files are read and written as **UTF-8**. A file in another encoding is not
opened (and never modified). The error message says where the invalid data
starts, so you can convert the file first. A UTF-8 byte order mark and the
file's line endings (LF, CRLF or CR) are preserved on save. The status bar
shows both.

### Safe saving

Saves are **atomic**: the new contents are written to a temporary file next
to the script and then swapped in. A crash or power loss in the middle of a
save leaves either the old or the new file, never a truncated one.

## Automatic reload and preview

Turn on **File > Automatic Reload and Preview** (toolbar ⟳) to edit the
script in your favorite editor while CQ-editor shows the result:

* the file is checked every 250 ms, and the script re-runs after it changes.
  Editors that save by deleting and recreating the file (vim, neovim) are
  supported;
* *Autoreload delay* (default 50 ms) waits for the other editor to finish
  writing;
* *Autoreload: watch imported modules* also re-runs when local modules
  imported by the script change;
* a save that only touches the timestamp, without changing the contents,
  does not re-run anything.

### When the file changes while you have unsaved edits

CQ-editor never silently throws away your edits. If the file on disk changes
while the buffer has unsaved changes that differ from it, the **File changed
on disk** dialog appears:

| Choice | Result |
| --- | --- |
| **Keep my version** | The buffer is unchanged. Saving later overwrites the disk version |
| **Reload from disk** | The disk version replaces the buffer. Your edits are first saved as a recovery backup, whose path is shown in the status bar and the log |
| **Save a copy…** | Saves your edits under another name (`<name>.local-copy.py` by default), then reloads the disk version |
| **Compare changes** | Shows both versions side by side before you decide |

If the disk version matches your buffer, nothing is lost and the reload
happens without a question.

## Crash recovery and local history

While a buffer has unsaved changes, CQ-editor writes a **recovery
checkpoint** every 5 seconds (and before risky operations such as reloading
or saving).

* **After a crash** or a forced shutdown, the next start shows **Unsaved work
  can be recovered**, with the time of the checkpoint. Choose **Restore
  session** or **Discard recovery**.
* **File > Local History…** lists earlier versions of the current document:
  checkpoints, saved versions and the contents a *Save As* replaced. Select
  one to preview it, then **Restore this version**. The newest **20
  versions per document** are kept.

Recovery data stays on your computer, in the `recovery/` folder of the
[application data folder](../getting-started/installation.md#where-cq-editor-keeps-its-files).
A normal save or a deliberate *Don't save* ends the recovery session.
