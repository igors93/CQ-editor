use super::*;

#[test]
fn specific_shortcuts_come_first() {
    let all = shortcuts();
    let position = |action: &Action| all.iter().position(|(a, _)| a == action).unwrap();
    assert!(position(&Action::View(StandardView::Front)) < position(&Action::Render));
    assert!(position(&Action::SaveAs) < position(&Action::Save));
    assert!(position(&Action::Stop) < position(&Action::ToggleDebug));
}

#[test]
fn menus_find_their_shortcuts() {
    assert_eq!(shortcut(&Action::Render), Some(key(Key::F5)));
    assert_eq!(shortcut(&Action::Autocomplete), Some(ctrl(Key::Space)));
    assert_eq!(shortcut(&Action::Upload), None);
}
