use super::*;

#[test]
fn finds_matches_with_and_without_case() {
    let text = "Box box BOX";
    assert_eq!(find_all(text, "box", true), vec![4..7]);
    assert_eq!(find_all(text, "box", false), vec![0..3, 4..7, 8..11]);
    assert_eq!(find_all("aé é", "É", false), vec![1..3, 4..6]);
    assert!(find_all(text, "", false).is_empty());
}

#[test]
fn typed_text_is_empty_after_moving_back() {
    assert_eq!(typed_since("cq.Wor", 3, 6), "Wor");
    assert_eq!(typed_since("cq.Wor", 3, 2), "");
}

#[test]
fn breakpoints_and_the_error_line_follow_inserted_and_deleted_lines() {
    let mut editor = CodeEditor::default();
    editor.sync_markers("a = 1\nb = 2\nc = 3\n");
    editor.breakpoints.extend([2, 3]);
    editor.error_line = Some(3);

    editor.sync_markers("import x\na = 1\nb = 2\nc = 3\n");
    assert_eq!(
        editor.breakpoints.iter().copied().collect::<Vec<_>>(),
        [3, 4]
    );
    assert_eq!(editor.error_line, Some(4));

    // Deleting the line under a breakpoint removes the breakpoint.
    editor.sync_markers("import x\na = 1\nc = 3\n");
    assert_eq!(editor.breakpoints.iter().copied().collect::<Vec<_>>(), [3]);
    assert_eq!(editor.error_line, Some(3));
}

#[test]
fn a_new_document_does_not_inherit_line_moves() {
    let mut editor = CodeEditor::default();
    editor.sync_markers("a\nb\nc\n");
    editor.reset();
    editor.breakpoints.insert(2);

    // The first text after a reset is the reference, not an edit.
    editor.sync_markers("x\n");
    assert_eq!(editor.breakpoints.iter().copied().collect::<Vec<_>>(), [2]);
}
