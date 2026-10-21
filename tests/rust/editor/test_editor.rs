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
