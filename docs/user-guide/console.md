# Console, variables and logs

## Console

The **Console** panel is an interactive Python prompt running in the same
Python process as your scripts.

* After each run it contains the script's **variables**, so you can query
  the model without editing the script:

  ```python
  >>> bracket.val().Volume()
  10944.0…
  >>> bracket.faces(">Z").val().Center()
  ```

* It offers the script API: `show_object` (or its alias `show`), `debug`,
  `log`, `rand_color` and `cq`. Objects shown from the console are **added**
  to the view, next to the script's objects:

  ```python
  >>> show_object(cq.Workplane().sphere(5), name="ball", color="tomato")
  ```

* `print` output and errors appear in the console itself.

| Key | Action |
| --- | --- |
| Enter | Run the input |
| Shift+Enter | New line (for multi-line statements such as `for` loops) |
| ↑ / ↓ | Browse the input history (single-line input) |
| Tab | Complete the name before the cursor (needs `jedi`) |

An incomplete statement (an open `for:` block, for example) waits for more
input, as in the standard Python prompt.

The buttons next to the input clear the console (**Run > Clear Console**
does the same) and restart it with a fresh namespace. **Run > Stop** and
**Run > Restart Python** also reset the console, since they start a new
Python process.

## Variables

The **Variables** panel lists name, type and value of every public variable
after a run, or of the local variables of the paused frame while debugging.

* Type in the filter box to search by name or type.
* Values are cut at 300 characters. Hover a value to read it when the column
  is too narrow.
* The names CQ-editor injects (`show_object`, `debug`, …) are hidden.

## Traceback

When a run fails, the **Traceback** panel shows:

* the exception type and message, in red;
* one row per frame (**File**, **Line**, **Function**, **Code**), starting
  at your script. Frames inside CQ-editor are hidden, frames inside CadQuery
  are kept;
* a copy button for the full Python traceback.

Click a row that belongs to your script to jump to that line. When the last
run succeeded, the panel shows *No errors*.

## Log

The **Log** panel collects everything that is not a direct result:

| Source | Examples |
| --- | --- |
| `print` | `print()` calls in scripts (stdout) |
| `stderr` | Text written to `sys.stderr` and native library messages (OpenCascade) |
| `script` | `log(value)` calls |
| Python loggers | `logging.info(...)`, warnings such as *Ignoring invalid display option* |
| CQ-editor | Worker start-up, recovery events, file reload messages |

* Toggle **Errors**, **Warnings**, **Info** and **Debug** to filter by level
  (Debug is hidden by default), or type in the filter box.
* The copy button copies the lines currently shown. **Clear** (or **Run >
  Clear Log**) empties the log.
* The log keeps the newest 10,000 lines.
