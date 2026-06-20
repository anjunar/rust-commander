use super::copy_tip;

#[test]
fn copy_tip_writes_nul_terminated_utf16() {
    let mut buffer = [0u16; 8];
    copy_tip(&mut buffer, "RC");
    assert_eq!(&buffer[..3], &[b'R' as u16, b'C' as u16, 0]);
}
