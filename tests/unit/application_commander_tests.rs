use std::{path::PathBuf, time::SystemTime};

use crate::{
    application::{FileOperationKind, OperationPlan},
    config::PanelSettings,
    domain::{Entry, EntryKind, PanelLocation},
    remote::{RemoteAuthConfig, RemotePath, RemoteProfile, RemoteRuntimeSecret, RemoteSession},
};

use super::{ActivePanel, Commander, SessionStore};

#[test]
fn delete_request_allows_remote_in_inactive_panel() {
    let mut commander = Commander::new(
        PathBuf::from("/tmp/left"),
        PathBuf::from("/tmp/right"),
        PanelSettings::default(),
        Vec::new(),
        "Ready".into(),
    );

    commander.state.active_panel = ActivePanel::Left;
    commander.state.left.location = PanelLocation::filesystem(PathBuf::from("/tmp/left"));
    commander.state.left.entries = vec![file_entry("keep.txt"), file_entry("delete.txt")];
    commander.state.left.select_single(1);
    let mut session_store = SessionStore::default();
    let session_key = session_store.insert_remote(remote_session());
    commander.state.right.location =
        PanelLocation::remote(session_key, "tester", "example.com", 22, "/home/test");

    let request = commander
        .operation_request(FileOperationKind::Delete, &session_store)
        .unwrap();

    match request {
        OperationPlan::Local(local) => {
            assert_eq!(local.sources, vec![PathBuf::from("/tmp/left/delete.txt")]);
            assert!(local.target_directory.is_none());
        }
        _ => panic!("expected local operation plan"),
    }
}

fn file_entry(name: &str) -> Entry {
    Entry {
        name: name.into(),
        archive_path: None,
        remote_path: None,
        kind: EntryKind::File,
        is_dir: false,
        size_bytes: 1,
        modified_at: Some(SystemTime::now()),
        attributes: String::new(),
        is_parent_link: false,
    }
}

fn remote_session() -> RemoteSession {
    RemoteSession::new(
        RemoteProfile {
            name: "test".into(),
            host: "example.com".into(),
            port: 22,
            auth: RemoteAuthConfig::Password {
                username: "tester".into(),
            },
            start_directory: RemotePath::new("/home/test"),
            skip_host_key_verification: false,
        },
        RemoteRuntimeSecret::Password("secret".into()),
    )
}
