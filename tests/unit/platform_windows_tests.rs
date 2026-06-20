use std::ffi::OsStr;

use super::to_wide;

#[test]
fn to_wide_appends_trailing_nul() {
    let wide = to_wide(OsStr::new("open"));
    assert_eq!(wide.last().copied(), Some(0));
    assert_eq!(
        wide[..wide.len() - 1],
        ['o' as u16, 'p' as u16, 'e' as u16, 'n' as u16]
    );
}
