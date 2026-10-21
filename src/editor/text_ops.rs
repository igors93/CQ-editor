//! Pure text operations behind the editor commands.
//!
//! Positions are byte offsets into the text. A selection is an
//! `(anchor, cursor)` pair; both are remapped through every edit so the
//! selection keeps covering the same text afterwards.

use std::ops::Range;

pub const INDENT: &str = "    ";
const INDENT_WIDTH: usize = 4;

/// Statements after which the next line is usually dedented.
const DEDENT_KEYWORDS: &[&str] = &["return", "pass", "break", "continue", "raise"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub cursor: usize,
}

impl Selection {
    pub fn caret(at: usize) -> Self {
        Self {
            anchor: at,
            cursor: at,
        }
    }

    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }
}

/// The result of an edit: the new text and the remapped selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edited {
    pub text: String,
    pub selection: Selection,
}

pub fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(i, _)| i)
}

pub fn byte_to_char(text: &str, byte_index: usize) -> usize {
    text[..byte_index.min(text.len())].chars().count()
}

/// 0-based line containing `byte`.
pub fn line_of(text: &str, byte: usize) -> usize {
    text.as_bytes()[..byte.min(text.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// Byte offset where 0-based `line` starts (the end of the text past the last line).
pub fn line_start(text: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    text.match_indices('\n')
        .nth(line - 1)
        .map_or(text.len(), |(i, _)| i + 1)
}

/// Byte range of 0-based `line`, without its line break.
pub fn line_range(text: &str, line: usize) -> Range<usize> {
    let start = line_start(text, line);
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    start..end
}

/// 0-based line and character column of `byte`.
pub fn line_column(text: &str, byte: usize) -> (usize, usize) {
    let line = line_of(text, byte);
    let start = line_start(text, line);
    (line, text[start..byte.min(text.len())].chars().count())
}

/// Lines touched by a selection. A selection ending at the very start of a
/// line does not include that line.
fn selected_lines(text: &str, selection: &Selection) -> Range<usize> {
    let range = selection.range();
    let first = line_of(text, range.start);
    let mut last = line_of(text, range.end);
    if last > first && range.end == line_start(text, last) {
        last -= 1;
    }
    first..last + 1
}

/// Applies non-overlapping replacements, sorted by position.
fn apply(text: &str, selection: &Selection, edits: &[(Range<usize>, String)]) -> Edited {
    let mut out = String::with_capacity(text.len() + edits.len() * INDENT_WIDTH);
    let mut last = 0;
    for (range, replacement) in edits {
        out.push_str(&text[last..range.start]);
        out.push_str(replacement);
        last = range.end;
    }
    out.push_str(&text[last..]);

    let map = |offset: usize| {
        let mut shift: isize = 0;
        for (range, replacement) in edits {
            if offset >= range.end {
                shift += replacement.len() as isize - range.len() as isize;
            } else if offset > range.start {
                // Inside a removed span: snap to its start.
                shift -= (offset - range.start) as isize;
            }
        }
        (offset as isize + shift).max(0) as usize
    };
    Edited {
        text: out,
        selection: Selection {
            anchor: map(selection.anchor),
            cursor: map(selection.cursor),
        },
    }
}

fn leading_whitespace(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// Comments the selected lines with `# `, or uncomments them when every
/// non-blank line is already a comment.
pub fn toggle_comment(text: &str, selection: &Selection) -> Edited {
    let lines: Vec<Range<usize>> = selected_lines(text, selection)
        .map(|line| line_range(text, line))
        .filter(|range| !text[range.clone()].trim().is_empty())
        .collect();
    if lines.is_empty() {
        return Edited {
            text: text.to_owned(),
            selection: selection.clone(),
        };
    }

    let all_commented = lines
        .iter()
        .all(|range| text[range.clone()].trim_start().starts_with('#'));
    let edits: Vec<(Range<usize>, String)> = if all_commented {
        lines
            .iter()
            .map(|range| {
                let line = &text[range.clone()];
                let hash = range.start + leading_whitespace(line).len();
                let end = if text[hash + 1..].starts_with(' ') {
                    hash + 2
                } else {
                    hash + 1
                };
                (hash..end, String::new())
            })
            .collect()
    } else {
        let column = lines
            .iter()
            .map(|range| leading_whitespace(&text[range.clone()]).len())
            .min()
            .unwrap_or(0);
        lines
            .iter()
            .map(|range| (range.start + column..range.start + column, "# ".to_owned()))
            .collect()
    };
    apply(text, selection, &edits)
}

/// Tab: indents every selected line when the selection spans several lines,
/// otherwise replaces the selection with spaces up to the next tab stop.
pub fn indent(text: &str, selection: &Selection) -> Edited {
    let lines = selected_lines(text, selection);
    if lines.len() > 1 {
        let edits: Vec<_> = lines
            .map(|line| line_range(text, line))
            .filter(|range| !range.is_empty())
            .map(|range| (range.start..range.start, INDENT.to_owned()))
            .collect();
        return apply(text, selection, &edits);
    }

    let range = selection.range();
    let (_, column) = line_column(text, range.start);
    let spaces = INDENT_WIDTH - column % INDENT_WIDTH;
    let mut edited = apply(text, selection, &[(range.clone(), " ".repeat(spaces))]);
    edited.selection = Selection::caret(range.start + spaces);
    edited
}

/// Shift+Tab: removes one level of indentation from the selected lines.
pub fn unindent(text: &str, selection: &Selection) -> Edited {
    let edits: Vec<_> = selected_lines(text, selection)
        .map(|line| line_range(text, line))
        .filter_map(|range| {
            let line = &text[range.clone()];
            let remove = if line.starts_with('\t') {
                1
            } else {
                let spaces = line.len() - line.trim_start_matches(' ').len();
                match spaces {
                    0 => return None,
                    n if n % INDENT_WIDTH == 0 => INDENT_WIDTH,
                    n => n % INDENT_WIDTH,
                }
            };
            Some((range.start..range.start + remove, String::new()))
        })
        .collect();
    apply(text, selection, &edits)
}

/// Line break plus indentation for Enter at `cursor`: keeps the current
/// indentation, adds a level after a trailing `:` and removes one after
/// `return`, `pass` and similar.
pub fn newline_with_indent(text: &str, cursor: usize) -> String {
    let line = line_of(text, cursor);
    let before_cursor = &text[line_start(text, line)..cursor];
    let indent = leading_whitespace(before_cursor);
    let code = strip_comment(before_cursor).trim_end();

    let mut new_indent = indent.to_owned();
    if code.ends_with(':') {
        new_indent.push_str(INDENT);
    } else {
        let first_word = code
            .trim_start()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next();
        if first_word.is_some_and(|w| DEDENT_KEYWORDS.contains(&w)) {
            let keep = new_indent.len().saturating_sub(INDENT_WIDTH);
            new_indent.truncate(keep);
        }
    }
    format!("\n{new_indent}")
}

/// The line without a trailing `#` comment (ignoring `#` inside strings).
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None if c == '#' => return &line[..i],
            None if c == '"' || c == '\'' => quote = Some(c),
            None => {}
        }
    }
    line
}

#[cfg(test)]
#[path = "../../tests/rust/editor/test_text_ops.rs"]
mod tests;
