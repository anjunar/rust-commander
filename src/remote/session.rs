use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::RemotePath;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RemoteConfig {
    pub profiles: Vec<RemoteProfile>,
    pub last_used_profile: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct RemoteProfile {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub auth: RemoteAuthConfig,
    pub start_directory: RemotePath,
}

impl Default for RemoteProfile {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: 22,
            auth: RemoteAuthConfig::Password {
                username: String::new(),
            },
            start_directory: RemotePath::root(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RemoteAuthConfig {
    Password {
        username: String,
    },
    KeyFile {
        username: String,
        private_key_path: PathBuf,
        public_key_path: Option<PathBuf>,
    },
}

impl RemoteAuthConfig {
    pub fn username(&self) -> &str {
        match self {
            Self::Password { username } | Self::KeyFile { username, .. } => username,
        }
    }
}

#[derive(Clone, Debug)]
pub enum RemoteRuntimeSecret {
    None,
    Password(String),
    KeyPassphrase(String),
}

#[derive(Clone, Debug)]
pub struct RemoteSession {
    profile: RemoteProfile,
    secret: RemoteRuntimeSecret,
}

impl RemoteSession {
    pub fn new(profile: RemoteProfile, secret: RemoteRuntimeSecret) -> Self {
        Self { profile, secret }
    }

    pub fn profile(&self) -> &RemoteProfile {
        &self.profile
    }

    pub fn secret(&self) -> &RemoteRuntimeSecret {
        &self.secret
    }

    pub fn start_directory(&self) -> RemotePath {
        self.profile.start_directory.clone()
    }
}

#[derive(Clone, Debug)]
pub struct RemoteLocation {
    pub session: RemoteSession,
    pub current_path: RemotePath,
}

impl RemoteLocation {
    pub fn new(session: RemoteSession, current_path: RemotePath) -> Self {
        Self {
            session,
            current_path,
        }
    }
}
