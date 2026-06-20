use std::collections::HashMap;

use crate::{archive::ArchiveSession, remote::RemoteSession};

#[derive(Clone, Debug, Default)]
pub struct SessionStore {
    next_id: u64,
    archives: HashMap<String, ArchiveSession>,
    remotes: HashMap<String, RemoteSession>,
}

impl SessionStore {
    pub fn insert_archive(&mut self, session: ArchiveSession) -> String {
        let key = self.next_key("archive");
        self.archives.insert(key.clone(), session);
        key
    }

    pub fn archive(&self, key: &str) -> Option<ArchiveSession> {
        self.archives.get(key).cloned()
    }

    pub fn insert_remote(&mut self, session: RemoteSession) -> String {
        let key = self.next_key("remote");
        self.remotes.insert(key.clone(), session);
        key
    }

    pub fn remote(&self, key: &str) -> Option<RemoteSession> {
        self.remotes.get(key).cloned()
    }

    fn next_key(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}-{}", self.next_id)
    }
}
