use std::{
    fs,
    sync::{atomic::AtomicBool, mpsc, Arc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use super::{
    build_copy_plan, copy_path, operation_failure, step, CopyProgress, FileOperationKind,
    OperationEvent, OperationPlan, PathAccess,
};

#[test]
fn cancelled_directory_copy_cleans_partially_created_target() {
    let test_root = unique_test_dir("copy_cancel_cleanup");
    let source_root = test_root.join("source");
    let nested = source_root.join("nested");
    let target_root = test_root.join("target").join("source");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("large.bin"), vec![7_u8; 256 * 1024 * 1024]).unwrap();

    let plan = OperationPlan {
        total_bytes: 256 * 1024 * 1024,
        total_entries: 3,
    };
    let mut progress = CopyProgress {
        processed_bytes: 0,
        processed_entries: 0,
    };
    let (tx, _rx) = mpsc::channel::<OperationEvent>();
    let (_resolution_tx, resolution_rx) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::clone(&cancelled);
    let target_watch = target_root.clone();

    let watcher = thread::spawn(move || {
        let child_dir = target_watch.join("nested");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if child_dir.exists() {
                cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!("Timed out waiting for partial target directory creation");
    });

    copy_path(
        &source_root,
        &target_root,
        &plan,
        &mut progress,
        Instant::now(),
        &tx,
        &FileOperationKind::Copy,
        &cancelled,
        &resolution_rx,
    )
    .unwrap();
    watcher.join().unwrap();

    assert!(!target_root.exists());
    let _ = fs::remove_dir_all(&test_root);
}

fn unique_test_dir(label: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("rust_commander_{label}_{suffix}"))
}

#[test]
fn a_refused_step_keeps_its_path_and_kind_of_access() {
    let path = std::path::Path::new("protected.txt");
    let error =
        anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::PermissionDenied)).context(
            step(PathAccess::Write, path, "Could not create target file"),
        );

    let failure = operation_failure(&error);

    assert_eq!(failure.denied(), Some((PathAccess::Write, path)));
}

#[test]
fn a_missing_path_is_no_refusal_and_says_why() {
    let root = unique_test_dir("missing_path");
    let missing = root.join("missing");
    let cause = fs::metadata(&missing).unwrap_err().to_string();

    let error = build_copy_plan(&missing).unwrap_err();

    let failure = operation_failure(&error);
    assert_eq!(failure.denied(), None);
    assert!(failure.detail().contains(&missing.display().to_string()));
    assert!(failure.detail().ends_with(&cause));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn any_other_failure_keeps_its_cause_in_the_detail() {
    let path = std::path::Path::new("busy.txt");
    let error = anyhow::Error::from(std::io::Error::other("the disk is on fire")).context(step(
        PathAccess::Write,
        path,
        "Could not write",
    ));

    let failure = operation_failure(&error);

    assert_eq!(failure.denied(), None);
    assert_eq!(
        failure.detail(),
        "Could not write busy.txt: the disk is on fire"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn delete_access_of_a_plain_file_and_folder_is_granted() {
    let root = unique_test_dir("delete_access");
    fs::create_dir_all(&root).unwrap();
    let file = root.join("file.txt");
    fs::write(&file, "x").unwrap();

    assert!(super::delete_access(&file).is_ok());
    assert!(super::delete_access(&root).is_ok());
    assert!(file.exists());
    let _ = fs::remove_dir_all(root);
}

#[cfg(target_os = "windows")]
mod shell_operation {
    use std::path::PathBuf;

    use crate::{
        application::{FileOperationKind, LocalOperationRequest},
        platform::windows_shell_operation::ShellAction,
    };

    use super::super::shell_operation;

    fn request(kind: FileOperationKind, sources: &[&str]) -> LocalOperationRequest {
        LocalOperationRequest {
            kind,
            sources: sources.iter().map(PathBuf::from).collect(),
            target_directory: Some(PathBuf::from("target")),
            target_path: None,
            use_recycle_bin: false,
        }
    }

    #[test]
    fn an_explicit_target_splits_into_folder_and_name() {
        let mut copy = request(FileOperationKind::Copy, &["a.txt"]);
        copy.target_path = Some(PathBuf::from("elsewhere").join("b.txt"));

        let operation = shell_operation(&copy, false, 0).unwrap();

        let ShellAction::Copy(target) = operation.action else {
            panic!("expected a copy");
        };
        assert_eq!(target.directory, PathBuf::from("elsewhere"));
        assert_eq!(target.name.as_deref(), Some("b.txt".as_ref()));
        assert!(operation.elevate);
    }

    #[test]
    fn an_explicit_target_does_not_rename_several_sources() {
        let mut transfer = request(FileOperationKind::Move, &["a.txt", "b.txt"]);
        transfer.target_path = Some(PathBuf::from("elsewhere").join("c.txt"));

        let operation = shell_operation(&transfer, true, 0).unwrap();

        let ShellAction::Move(target) = operation.action else {
            panic!("expected a move");
        };
        assert_eq!(target.directory, PathBuf::from("target"));
        assert_eq!(target.name, None);
        assert!(target.overwrite_silently);
    }

    #[test]
    fn delete_keeps_the_choice_of_the_recycle_bin() {
        let mut delete = request(FileOperationKind::Delete, &["a.txt"]);
        delete.use_recycle_bin = true;

        let operation = shell_operation(&delete, false, 0).unwrap();

        assert!(matches!(
            operation.action,
            ShellAction::Delete { recycle: true }
        ));
    }
}
