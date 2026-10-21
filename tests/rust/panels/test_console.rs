use super::*;

#[test]
fn completion_inserts_unique_matches_and_common_prefixes() {
    let mut console = ConsoleState {
        input: "cq.Work".to_owned(),
        ..Default::default()
    };
    console.complete(&[("Workplane".into(), "plane".into())], &[]);
    assert_eq!(console.input, "cq.Workplane");

    let mut console = ConsoleState::default();
    console.complete(
        &[
            ("box".into(), "ox".into()),
            ("bounding".into(), "ounding".into()),
        ],
        &[],
    );
    assert_eq!(console.input, "o");

    let mut console = ConsoleState::default();
    console.complete(
        &[("abc".into(), "abc".into()), ("xyz".into(), "xyz".into())],
        &[],
    );
    assert_eq!(console.entries[0].text, "abc  xyz\n");
}
