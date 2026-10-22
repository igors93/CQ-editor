# Code editor

The editor is built for short CadQuery scripts: Python highlighting, smart
indentation, completion, documentation on hover, search and replace, and
breakpoints. It also takes care that your work is never lost.

## Editing

| Feature | How |
| --- | --- |
| Syntax highlighting | Automatic. Colors follow the light or dark theme |
| Line numbers, current line | Always shown. The current-line highlight can be turned off |
| Vertical ruler | At column 79 by default (*Maximum line length*, 0 hides it) |
| Indent / unindent | Tab / Shift+Tab. With several lines selected, Tab indents them all. Indentation is 4 spaces |
| Auto-indent | Enter keeps the indentation, indents after a line ending with `:` and dedents after `return`, `pass`, `break`, `continue` or `raise` |
| Toggle comment | Ctrl+/ comments or uncomments the selected lines |
| Undo / redo | Ctrl+Z / Ctrl+Y (or Ctrl+Shift+Z), also in the **Edit** menu. Typing is undone a burst at a time: a pause, a new line or a click elsewhere starts the next step. An indent, a comment, an accepted completion, a *Replace all* and a reload from disk are each one step. The history belongs to the open document |
| Go to line | Ctrl+G opens a bar above the editor: type a line, or `line:column`, and press Enter |
| Line wrap | Off by default (*Preferences > Editor*) |
| Font size | 13 by default (8–28) |

## Completion and call signatures

Completion opens by itself after you type `.` (for example `cq.` or
`result.`), and once you have typed the first three characters of a name
(*Preferences > Editor > Complete names after*; 0 turns that off). Press
**Ctrl+Space** (or **Alt+/**) to complete the name under the cursor at any
other time:

* a popup lists matching names (up to 200) and filters as you keep typing;
* **↑/↓** choose, **Enter** or **Tab** accept, **Esc** closes;
* right after `(` or `,`, the call signature of the function is shown
  instead, for example the parameters of `Workplane.box`.

Completion runs in the Python worker with [jedi](https://github.com/davidhalter/jedi).
It knows the script API (`show_object`, `cq`, …) and anything importable from
the selected environment. Install `jedi` in that environment to enable it.
Without jedi, the command does nothing.

## Documentation on hover

Rest the mouse pointer on a name for a moment to see its documentation in a
tooltip: the call signature and docstring of a function or class (for
example `Workplane.box`), the docstring of a module, or the type a variable
holds (`box: Workplane`). It also comes from jedi, and appears only while
no script is running.

## Search

**Ctrl+F** opens the search bar above the editor:

* matches are highlighted as you type, and the bar shows *N of M* (or *No
  matches*);
* **Enter** / **Shift+Enter** go to the next / previous match after the
  cursor. From the editor, use **F3** / **Alt+F3**;
* **Aa** toggles *Match case*, and **ab** *Whole word*, which skips the
  term inside longer names (`box` but not `boxes`);
* **Esc** closes the bar.

**Ctrl+H** (or the ⇄ button of the bar) shows the replace row. **Replace**
(or Enter in its field) replaces the selected match and selects the next
one; when no match is selected it first selects the next one, so nothing is
replaced unseen. **Replace all** replaces every match at once, as a single
step to undo.

## Breakpoints and markers

Click a line number to toggle a **breakpoint** (red dot). Breakpoints stay
on their statements while you edit: adding or removing lines above one moves
it along, and deleting its line removes it.

Breakpoints are **remembered per file** across sessions (for the last 100
files). Each is saved with the text of its line, so if the file changed in
another editor meanwhile, it returns to the line with that text, or is
dropped when no line has it any more. The gutter and line backgrounds also
show:

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

Files are read and written as **UTF-8**. When a file is not valid UTF-8,
CQ-editor asks before doing anything: **Open as Windows-1252** reads it with
the Western European encoding of older Windows editors, without changing the
file. Saving it then writes UTF-8, the encoding Python expects; until then
the status bar shows *Windows-1252 → UTF-8*. **Cancel** leaves the file
alone. A UTF-8 byte order mark and the file's line endings (LF, CRLF or CR)
are preserved on save. The status bar shows the encoding and the line
endings.

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
  versions per document** are kept. Versions older than 90 days are removed,
  which also clears the history of files you deleted or renamed, and the
  oldest versions go first when the whole history exceeds 64 MB.

Recovery data stays on your computer, in the `recovery/` folder of the
[application data folder](../getting-started/installation.md#where-cq-editor-keeps-its-files).
A normal save or a deliberate *Don't save* ends the recovery session.
