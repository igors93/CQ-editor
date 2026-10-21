use super::*;

fn sel(anchor: usize, cursor: usize) -> Selection {
    Selection { anchor, cursor }
}

#[test]
fn line_helpers() {
    let text = "ab\ncd\n\nef";
    assert_eq!(line_of(text, 4), 1);
    assert_eq!(line_start(text, 2), 6);
    assert_eq!(line_range(text, 3), 7..9);
    assert_eq!(line_column("é\nxyz", 5), (1, 2));
    assert_eq!(char_to_byte("aé b", 2), 3);
    assert_eq!(byte_to_char("aé b", 3), 2);
}

#[test]
fn comments_a_single_line_and_moves_the_caret() {
    let edited = toggle_comment("    x = 1\n", &Selection::caret(6));
    assert_eq!(edited.text, "    # x = 1\n");
    assert_eq!(edited.selection, Selection::caret(8));
    let back = toggle_comment(&edited.text, &edited.selection);
    assert_eq!(back.text, "    x = 1\n");
    assert_eq!(back.selection, Selection::caret(6));
}

#[test]
fn comments_blocks_at_the_common_indentation() {
    let text = "if a:\n    b = 1\n\n    # c\n";
    let edited = toggle_comment(text, &sel(0, text.len()));
    assert_eq!(edited.text, "# if a:\n#     b = 1\n\n#     # c\n");
    // All lines commented now, so toggling again uncomments them.
    let back = toggle_comment(&edited.text, &sel(0, edited.text.len()));
    assert_eq!(back.text, text);
}

#[test]
fn selection_ending_at_line_start_excludes_that_line() {
    let text = "a\nb\nc\n";
    let edited = toggle_comment(text, &sel(0, 4));
    assert_eq!(edited.text, "# a\n# b\nc\n");
}

#[test]
fn indents_to_tab_stops_or_whole_lines() {
    let edited = indent("ab", &Selection::caret(1));
    assert_eq!(edited.text, "a   b");
    assert_eq!(edited.selection, Selection::caret(4));

    let edited = indent("a\n\nb\n", &sel(0, 4));
    assert_eq!(edited.text, "    a\n\n    b\n");
    // Insertions at a selection end push it forward.
    assert_eq!(edited.selection, sel(4, 12));
}

#[test]
fn unindents_to_the_previous_tab_stop() {
    let edited = unindent("      a\n\tb\nc", &sel(0, 11));
    assert_eq!(edited.text, "    a\nb\nc");
    let edited = unindent("        x", &Selection::caret(9));
    assert_eq!(edited.text, "    x");
    assert_eq!(edited.selection, Selection::caret(5));
}

#[test]
fn newline_indentation() {
    assert_eq!(newline_with_indent("def f(x):", 9), "\n    ");
    assert_eq!(newline_with_indent("    x = 1", 9), "\n    ");
    assert_eq!(newline_with_indent("    return x", 12), "\n");
    assert_eq!(
        newline_with_indent("for c in 'a:b':  # loop:", 24),
        "\n    "
    );
    assert_eq!(newline_with_indent("x = 'a#:'", 9), "\n");
}

#[test]
fn unicode_offsets_round_trip_at_character_boundaries() {
    let text = "á中\n🙂x";
    for index in 0..=text.chars().count() {
        let byte = char_to_byte(text, index);
        assert_eq!(byte_to_char(text, byte), index);
    }
    assert_eq!(line_column(text, char_to_byte(text, 4)), (1, 1));
    assert_eq!(char_to_byte(text, 100), text.len());
}

#[test]
fn commenting_blank_lines_is_a_noop() {
    let text = "  \n\n";
    let selection = sel(0, text.len());
    assert_eq!(
        toggle_comment(text, &selection),
        Edited {
            text: text.into(),
            selection,
        }
    );
}

#[test]
fn uncommenting_hash_without_space_only_removes_hash() {
    let edited = toggle_comment("  #value\n", &Selection::caret(4));
    assert_eq!(edited.text, "  value\n");
    assert_eq!(edited.selection, Selection::caret(3));
}

#[test]
fn indent_replaces_single_line_selection_at_next_tab_stop() {
    let edited = indent("abCD", &sel(2, 4));
    assert_eq!(edited.text, "ab  ");
    assert_eq!(edited.selection, Selection::caret(4));
}

#[test]
fn unindent_leaves_unindented_text_unchanged() {
    let text = "first\nsecond";
    let selection = sel(0, text.len());
    assert_eq!(
        unindent(text, &selection),
        Edited {
            text: text.into(),
            selection,
        }
    );
}

#[test]
fn newline_dedents_after_control_flow_statements() {
    for keyword in ["pass", "break", "continue", "raise", "return"] {
        let line = format!("        {keyword}");
        assert_eq!(newline_with_indent(&line, line.len()), "\n    ");
    }
    assert_eq!(newline_with_indent("    return_value", 16), "\n    ");
}

#[test]
fn newline_ignores_colons_inside_comments_and_strings() {
    let comment = "    value = 1  # note:";
    assert_eq!(newline_with_indent(comment, comment.len()), "\n    ");
    let string = "    label = 'x:'";
    assert_eq!(newline_with_indent(string, string.len()), "\n    ");
    let block = "    if ready: # note";
    assert_eq!(newline_with_indent(block, block.len()), "\n        ");
}

#[test]
fn line_map_keeps_lines_while_typing_inside_a_line() {
    let map = LineMap::new("a\nb\nc", "a\nbx\nc");
    assert_eq!([1, 2, 3].map(|l| map.map(l)), [Some(1), Some(2), Some(3)]);
}

#[test]
fn line_map_moves_lines_below_inserted_lines() {
    let map = LineMap::new("a\nb\nc", "a\nnew\nb\nc");
    assert_eq!([1, 2, 3].map(|l| map.map(l)), [Some(1), Some(3), Some(4)]);

    // Enter at the start of a line pushes it down; in its middle it stays.
    assert_eq!(LineMap::new("x\ny", "x\n\ny").map(2), Some(3));
    let split = LineMap::new("x = 1\ny", "x =\n 1\ny");
    assert_eq!((split.map(1), split.map(2)), (Some(1), Some(3)));
}

#[test]
fn line_map_follows_the_statement_rather_than_similar_text() {
    // The new line resembles the old one; the old line still moves down.
    let map = LineMap::new("y\nx = 1\nz", "y\nx = 0\nx = 1\nz");
    assert_eq!(map.map(2), Some(3));
}

#[test]
fn line_map_drops_deleted_lines_and_keeps_merged_ones() {
    let deleted = LineMap::new("a\nb\nc\nd", "a\nd");
    assert_eq!(
        [1, 2, 3, 4].map(|l| deleted.map(l)),
        [Some(1), None, None, Some(2)]
    );

    let merged = LineMap::new("a\nb\nc", "ab\nc");
    assert_eq!(
        [1, 2, 3].map(|l| merged.map(l)),
        [Some(1), Some(1), Some(2)]
    );
}

#[test]
fn line_map_keeps_positions_when_a_block_is_indented() {
    let map = LineMap::new("if x:\na\nb\nc", "if x:\n    a\n    b\nc");
    assert_eq!(
        [1, 2, 3, 4].map(|l| map.map(l)),
        [Some(1), Some(2), Some(3), Some(4)]
    );
}
