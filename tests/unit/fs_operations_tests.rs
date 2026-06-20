use std::{
    fs,
    sync::{atomic::AtomicBool, mpsc, Arc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use super::{copy_path, CopyProgress, FileOperationKind, OperationEvent, OperationPlan};

#[test]
fn cancelled_directory_copy_cleans_partially_created_target() {
    let test_root = unique_test_dir("copy_cancel_cleanup");
    let source_root = test_root.join("source");
    let nested = source_root.join("nested");
    let target_root = test_root.join("target").join("source");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("large.bin"), vec![7_u8; 64 * 1024 * 1024]).unwrap();

    let plan = OperationPlan {
        total_bytes: 64 * 1024 * 1024,
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
        let child_file = target_watch.join("nested").join("large.bin");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if child_file.exists() {
                cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("Timed out waiting for partial target file creation");
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
