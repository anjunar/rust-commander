use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RemotePath(String);

impl RemotePath {
    pub fn root() -> Self {
        Self("/".into())
    }

    pub fn new(path: impl AsRef<str>) -> Self {
        Self(normalize_remote_path(path.as_ref()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }

    pub fn join(&self, child: impl AsRef<str>) -> Self {
        let child = child.as_ref();
        if child.is_empty() {
            return self.clone();
        }
        if child.starts_with('/') {
            return Self::new(child);
        }
        if self.is_root() {
            Self::new(format!("/{child}"))
        } else {
            Self::new(format!("{}/{}", self.0, child))
        }
    }

    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            return None;
        }

        let trimmed = self.0.trim_end_matches('/');
        match trimmed.rsplit_once('/') {
            Some(("", _)) | None => Some(Self::root()),
            Some((parent, _)) => Some(Self(parent.to_string())),
        }
    }

    pub fn file_name(&self) -> Option<&str> {
        if self.is_root() {
            None
        } else {
            self.0.rsplit('/').next()
        }
    }
}

impl Default for RemotePath {
    fn default() -> Self {
        Self::root()
    }
}

impl fmt::Display for RemotePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn normalize_remote_path(path: &str) -> String {
    let mut parts = Vec::new();
    let replaced = path.replace('\\', "/");
    for part in replaced.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value),
        }
    }

    if parts.is_empty() {
        "/".into()
    } else {
        format!("/{}", parts.join("/"))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/remote_path_tests.rs"]
mod tests;
