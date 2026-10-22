use super::*;
use std::io::{BufReader, Cursor};

#[test]
fn limited_line_reader_caps_memory_and_resumes_at_the_next_line() {
    let input = Cursor::new(b"123456\nok\nlast".to_vec());
    let mut reader = BufReader::with_capacity(3, input);

    assert_eq!(
        read_limited_line(&mut reader, 4).unwrap(),
        LimitedLine::TooLong(b"1234".to_vec())
    );
    assert_eq!(
        read_limited_line(&mut reader, 4).unwrap(),
        LimitedLine::Line(b"ok".to_vec())
    );
    assert_eq!(
        read_limited_line(&mut reader, 4).unwrap(),
        LimitedLine::Line(b"last".to_vec())
    );
    assert_eq!(read_limited_line(&mut reader, 4).unwrap(), LimitedLine::Eof);
}

#[test]
fn exact_line_limit_is_accepted() {
    let mut reader = Cursor::new(b"1234\n".to_vec());
    assert_eq!(
        read_limited_line(&mut reader, 4).unwrap(),
        LimitedLine::Line(b"1234".to_vec())
    );
}

#[test]
fn mesh_messages_have_a_separate_larger_limit() {
    let control = br#"{"type":"output","text":"hello"}"#;
    let run = format!(
        r#"{{"id":1,"type":"run_finished","padding":"{}"}}"#,
        "x".repeat(MAX_CONTROL_MESSAGE_BYTES)
    );
    let inspection = format!(
        r#"{{"type":"inspection_geometry","padding":"{}"}}"#,
        "x".repeat(MAX_CONTROL_MESSAGE_BYTES)
    );

    assert_eq!(worker_message_limit(control), MAX_CONTROL_MESSAGE_BYTES);
    assert_eq!(worker_message_limit(run.as_bytes()), MAX_MESH_MESSAGE_BYTES);
    assert_eq!(
        worker_message_limit(inspection.as_bytes()),
        MAX_MESH_MESSAGE_BYTES
    );
}

#[test]
fn a_large_unknown_or_malformed_message_keeps_the_control_limit() {
    let unknown = format!(
        r#"{{"type":"future","data":"{}"}}"#,
        "x".repeat(2 * 1024 * 1024)
    );
    let malformed = vec![b'x'; MAX_CONTROL_MESSAGE_BYTES + 1];

    assert_eq!(
        worker_message_limit(unknown.as_bytes()),
        MAX_CONTROL_MESSAGE_BYTES
    );
    assert!(unknown.len() > worker_message_limit(unknown.as_bytes()));
    assert_eq!(worker_message_limit(&malformed), MAX_CONTROL_MESSAGE_BYTES);

    let deceptive_output = format!(
        r#"{{"type":"output","text":"\"type\":\"run_finished\"{}"}}"#,
        "x".repeat(MAX_CONTROL_MESSAGE_BYTES)
    );
    assert_eq!(
        worker_message_limit(deceptive_output.as_bytes()),
        MAX_CONTROL_MESSAGE_BYTES
    );
}
