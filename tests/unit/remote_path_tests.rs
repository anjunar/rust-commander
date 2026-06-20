use super::RemotePath;

#[test]
fn normalizes_root_and_segments() {
    assert_eq!(RemotePath::new("").as_str(), "/");
    assert_eq!(RemotePath::new("/var//log/./").as_str(), "/var/log");
    assert_eq!(RemotePath::new(r"\srv\share\..\logs").as_str(), "/srv/logs");
    assert_eq!(RemotePath::new("/../../tmp").as_str(), "/tmp");
}

#[test]
fn joins_children_and_absolute_paths() {
    assert_eq!(RemotePath::root().join("etc").as_str(), "/etc");
    assert_eq!(
        RemotePath::new("/etc").join("ssh/sshd_config").as_str(),
        "/etc/ssh/sshd_config"
    );
    assert_eq!(RemotePath::new("/etc").join("/usr/bin").as_str(), "/usr/bin");
}

#[test]
fn returns_parent_without_crossing_root() {
    assert_eq!(RemotePath::new("/etc/ssh").parent().unwrap().as_str(), "/etc");
    assert_eq!(RemotePath::new("/etc").parent().unwrap().as_str(), "/");
    assert!(RemotePath::root().parent().is_none());
}
