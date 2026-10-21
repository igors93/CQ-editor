use super::*;

#[test]
fn titles_drop_numbering_and_capitalize() {
    assert_eq!(example_title("01_hello_box.py"), "Hello Box");
    assert_eq!(
        example_title("selectors_and_fillets.py"),
        "Selectors And Fillets"
    );
    assert_eq!(example_title("loft.py"), "Loft");
}

#[test]
fn everything_needed_is_embedded() {
    let names: Vec<_> = WORKER_FILES.iter().map(|(name, _)| *name).collect();
    assert!(names.contains(&"cq_worker/__main__.py"));
    assert!(names.contains(&"cq_worker/server.py"));
    assert!(!examples().is_empty());
}
