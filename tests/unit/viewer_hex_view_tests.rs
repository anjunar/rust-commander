use super::render_hex_line;

#[test]
fn renders_short_hex_line() {
    let rendered = render_hex_line(0, b"Hello\n");
    assert_eq!(
        rendered,
        "00000000  48 65 6C 6C 6F 0A                                 |Hello.          |"
    );
}
