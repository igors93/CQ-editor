use super::*;

#[test]
fn strips_color_codes() {
    assert_eq!(strip_ansi("\x1b[31mred\x1b[0m text"), "red text");
    assert_eq!(strip_ansi("plain"), "plain");
}

#[test]
fn splits_multiline_messages() {
    let before = with_entries(|e| e.len());
    push(Level::Info, "test", "one\ntwo\r\n");
    let lines: Vec<String> =
        with_entries(|e| e.iter().skip(before).map(|e| e.text.clone()).collect());
    assert!(lines.ends_with(&["one".to_owned(), "two".to_owned()]));
}
