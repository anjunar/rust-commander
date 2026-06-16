use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootLocation {
    pub label: String,
    pub path: PathBuf,
}
