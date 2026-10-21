//! Python syntax highlighting for the code editor.
//!
//! A small hand-written lexer: it understands strings (prefixes, triple
//! quotes, escapes), comments, numbers, decorators and the names following
//! `def` and `class`, which is all highlighting needs.

use std::ops::Range;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    /// `True`, `False`, `None`.
    Constant,
    Builtin,
    /// The names CQ-editor injects into scripts (`show_object`, `cq`, ...).
    EditorApi,
    SelfParam,
    String,
    Number,
    Comment,
    Decorator,
    FunctionName,
    ClassName,
    Operator,
    Bracket,
    Identifier,
    Whitespace,
}

const KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda",
    "match", "case", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with",
    "yield",
];

const CONSTANTS: &[&str] = &["True", "False", "None"];

const BUILTINS: &[&str] = &[
    "abs",
    "all",
    "any",
    "bool",
    "dict",
    "dir",
    "enumerate",
    "filter",
    "float",
    "format",
    "getattr",
    "hasattr",
    "int",
    "isinstance",
    "iter",
    "len",
    "list",
    "map",
    "max",
    "min",
    "next",
    "object",
    "open",
    "print",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "sorted",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "zip",
    "Exception",
    "ValueError",
    "TypeError",
    "RuntimeError",
    "KeyError",
    "IndexError",
];

const EDITOR_API: &[&str] = &["show_object", "debug", "log", "rand_color", "cq"];

/// Colors for each token kind.
#[derive(Clone, Debug, PartialEq)]
pub struct SyntaxColors {
    pub text: Color32,
    pub keyword: Color32,
    pub constant: Color32,
    pub builtin: Color32,
    pub editor_api: Color32,
    pub self_param: Color32,
    pub string: Color32,
    pub number: Color32,
    pub comment: Color32,
    pub decorator: Color32,
    pub function: Color32,
    pub class: Color32,
    pub operator: Color32,
}

impl SyntaxColors {
    fn color(&self, kind: TokenKind) -> Color32 {
        match kind {
            TokenKind::Keyword => self.keyword,
            TokenKind::Constant => self.constant,
            TokenKind::Builtin => self.builtin,
            TokenKind::EditorApi => self.editor_api,
            TokenKind::SelfParam => self.self_param,
            TokenKind::String => self.string,
            TokenKind::Number => self.number,
            TokenKind::Comment => self.comment,
            TokenKind::Decorator => self.decorator,
            TokenKind::FunctionName => self.function,
            TokenKind::ClassName => self.class,
            TokenKind::Operator | TokenKind::Bracket => self.operator,
            TokenKind::Identifier | TokenKind::Whitespace => self.text,
        }
    }
}

/// Splits `text` into tokens covering it completely.
pub fn tokenize(text: &str) -> Vec<(Range<usize>, TokenKind)> {
    let bytes = text.as_bytes();
    let mut tokens: Vec<(Range<usize>, TokenKind)> = Vec::new();
    let mut i = 0;
    // What the previous identifier-like token makes of the next name.
    let mut expect_name: Option<TokenKind> = None;
    let mut line_start = true;

    while i < bytes.len() {
        let start = i;
        let c = bytes[i];

        if matches!(c, b' ' | b'\t' | b'\r' | b'\n') {
            while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
                line_start |= bytes[i] == b'\n';
                i += 1;
            }
            tokens.push((start..i, TokenKind::Whitespace));
            continue;
        }

        // A name right after `def`/`class`; any other token cancels it.
        let pending_name = expect_name.take();
        let kind = if c == b'#' {
            i = line_end(bytes, i);
            TokenKind::Comment
        } else if let Some(end) = string_end(bytes, i) {
            i = end;
            TokenKind::String
        } else if c.is_ascii_digit()
            || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            i = number_end(bytes, i);
            TokenKind::Number
        } else if c == b'@' && line_start {
            i += 1;
            while i < bytes.len() && (is_ident_byte(bytes[i]) || bytes[i] == b'.') {
                i += 1;
            }
            TokenKind::Decorator
        } else if is_ident_start(text, i) {
            i = ident_end(text, i);
            let word = &text[start..i];
            let kind = if let Some(kind) = pending_name {
                kind
            } else if KEYWORDS.contains(&word) {
                TokenKind::Keyword
            } else if CONSTANTS.contains(&word) {
                TokenKind::Constant
            } else if word == "self" || word == "cls" {
                TokenKind::SelfParam
            } else if EDITOR_API.contains(&word) {
                TokenKind::EditorApi
            } else if BUILTINS.contains(&word) && !follows_dot(bytes, start) {
                TokenKind::Builtin
            } else {
                TokenKind::Identifier
            };
            match word {
                "def" => expect_name = Some(TokenKind::FunctionName),
                "class" => expect_name = Some(TokenKind::ClassName),
                _ => {}
            }
            kind
        } else if matches!(c, b'(' | b')' | b'[' | b']' | b'{' | b'}') {
            i += 1;
            TokenKind::Bracket
        } else {
            // Any other character, taking care of multi-byte UTF-8.
            i += text[i..].chars().next().map_or(1, char::len_utf8);
            TokenKind::Operator
        };

        line_start = false;
        tokens.push((start..i, kind));
    }
    tokens
}

/// Builds the layout job for the editor: highlighted, monospace, wrapping
/// at `wrap_width` (infinite to disable wrapping).
pub fn layout_job(text: &str, font: FontId, colors: &SyntaxColors, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob {
        text: text.to_owned(),
        ..Default::default()
    };
    job.wrap.max_width = wrap_width;
    for (range, kind) in tokenize(text) {
        let format = TextFormat {
            font_id: font.clone(),
            color: colors.color(kind),
            italics: kind == TokenKind::Comment,
            ..Default::default()
        };
        job.sections.push(egui::text::LayoutSection {
            leading_space: 0.0,
            byte_range: egui::text::ByteIndex(range.start)..egui::text::ByteIndex(range.end),
            format,
        });
    }
    job
}

fn line_end(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn follows_dot(bytes: &[u8], start: usize) -> bool {
    bytes[..start]
        .iter()
        .rev()
        .find(|b| !b.is_ascii_whitespace())
        == Some(&b'.')
}

/// If a string literal (with optional prefix) starts at `i`, returns the
/// index just past it. Unterminated single-quoted strings stop at the end of
/// the line, unterminated triple-quoted ones at the end of the text.
fn string_end(bytes: &[u8], i: usize) -> Option<usize> {
    let mut j = i;
    while j < bytes.len()
        && j - i < 2
        && matches!(bytes[j].to_ascii_lowercase(), b'r' | b'b' | b'f' | b'u')
    {
        j += 1;
    }
    let quote = *bytes.get(j)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    // A prefix must not be the tail of a longer identifier.
    if j > i && i > 0 && is_ident_byte(bytes[i - 1]) {
        return None;
    }
    let triple = bytes.get(j + 1) == Some(&quote) && bytes.get(j + 2) == Some(&quote);
    let mut k = j + if triple { 3 } else { 1 };
    while k < bytes.len() {
        match bytes[k] {
            // Even in raw strings a backslash keeps the next quote inside.
            b'\\' => k += 2,
            b'\n' if !triple => return Some(k),
            c if c == quote => {
                if !triple {
                    return Some(k + 1);
                }
                if bytes.get(k + 1) == Some(&quote) && bytes.get(k + 2) == Some(&quote) {
                    return Some(k + 3);
                }
                k += 1;
            }
            _ => k += 1,
        }
    }
    Some(bytes.len())
}

fn number_end(bytes: &[u8], mut i: usize) -> usize {
    let mut previous = 0u8;
    while i < bytes.len() {
        let c = bytes[i];
        let sign_in_exponent = (c == b'+' || c == b'-') && matches!(previous, b'e' | b'E');
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || sign_in_exponent {
            previous = c;
            i += 1;
        } else {
            break;
        }
    }
    i
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

fn is_ident_start(text: &str, i: usize) -> bool {
    text[i..]
        .chars()
        .next()
        .is_some_and(|c| c == '_' || c.is_alphabetic())
}

fn ident_end(text: &str, i: usize) -> usize {
    text[i..]
        .char_indices()
        .find(|(_, c)| !(*c == '_' || c.is_alphanumeric()))
        .map_or(text.len(), |(offset, _)| i + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(&str, TokenKind)> {
        tokenize(text)
            .into_iter()
            .filter(|(_, kind)| *kind != TokenKind::Whitespace)
            .map(|(range, kind)| (&text[range], kind))
            .collect()
    }

    #[test]
    fn covers_the_whole_text() {
        let text = "x = f'a{b}' # hé\n\"\"\"doc\nmore\"\"\" 1.5e-3";
        let tokens = tokenize(text);
        assert_eq!(tokens.first().unwrap().0.start, 0);
        assert_eq!(tokens.last().unwrap().0.end, text.len());
        for pair in tokens.windows(2) {
            assert_eq!(pair[0].0.end, pair[1].0.start);
        }
    }

    #[test]
    fn recognizes_python_constructs() {
        use TokenKind::*;
        assert_eq!(
            kinds("def box(self, size=1.5):  # comment"),
            [
                ("def", Keyword),
                ("box", FunctionName),
                ("(", Bracket),
                ("self", SelfParam),
                (",", Operator),
                ("size", Identifier),
                ("=", Operator),
                ("1.5", Number),
                (")", Bracket),
                (":", Operator),
                ("# comment", Comment),
            ]
        );
        assert_eq!(kinds("class Part:")[1], ("Part", ClassName));
        assert_eq!(
            kinds("@cq.Workplane.method")[0],
            ("@cq.Workplane.method", Decorator)
        );
        assert_eq!(kinds("show_object(result)")[0], ("show_object", EditorApi));
        assert_eq!(kinds("x.print()")[2], ("print", Identifier));
        assert_eq!(kinds("None")[0], ("None", Constant));
    }

    #[test]
    fn handles_strings() {
        use TokenKind::*;
        assert_eq!(
            kinds(r#"rb'\d' "a\"b""#),
            [(r"rb'\d'", String), (r#""a\"b""#, String)]
        );
        assert_eq!(kinds("'''one\ntwo''' x")[0].1, String);
        assert_eq!(kinds("'open\nx")[0], ("'open", String));
        // An identifier ending in a prefix letter is not a string prefix.
        assert_eq!(kinds("bar'x'")[0], ("bar", Identifier));
    }

    #[test]
    fn handles_numbers() {
        use TokenKind::*;
        assert_eq!(
            kinds("0x1F 1_000 2.5e+10 .5 3j"),
            [
                ("0x1F", Number),
                ("1_000", Number),
                ("2.5e+10", Number),
                (".5", Number),
                ("3j", Number),
            ]
        );
    }
}
