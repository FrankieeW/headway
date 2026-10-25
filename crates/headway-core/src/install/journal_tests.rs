use super::*;

#[test]
fn interrupted_swap_and_commit_are_durable_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("source/Demo.app");
    let dest = dir.path().join("apps/Demo.app");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("value"), "new").unwrap();
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("value"), "old").unwrap();
    let tx = Transaction::begin(&src, &dest).unwrap();
    // Exact filesystem state at the crash window between the two renames.
    fs::rename(&dest, tx.staged()).unwrap();
    tx.defer();
    assert!(!dest.exists());
    reconcile(&dest, false).unwrap();
    reconcile(&dest, false).unwrap();
    assert_eq!(fs::read_to_string(dest.join("value")).unwrap(), "old");
    Transaction::begin(&src, &dest).unwrap().defer();
    reconcile(&dest, true).unwrap();
    reconcile(&dest, false).unwrap();
    assert_eq!(fs::read_to_string(dest.join("value")).unwrap(), "new");
}

#[test]
fn empty_reservation_is_reconciled_but_rescue_without_intent_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("tool");
    fs::write(&dest, "keep").unwrap();
    let journal = root(&dest).unwrap();
    fs::create_dir(&journal).unwrap();
    reconcile(&dest, false).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"keep");
    fs::create_dir_all(journal.join("old")).unwrap();
    fs::write(journal.join("old/tool"), "rescue").unwrap();
    assert!(reconcile(&dest, false).is_err());
    assert_eq!(fs::read(journal.join("old/tool")).unwrap(), b"rescue");
}

#[test]
fn malicious_journal_paths_and_symlinks_cannot_delete_outside_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("tool");
    let outside = dir.path().join("outside");
    fs::write(&outside, "keep").unwrap();
    let journal = root(&dest).unwrap();
    fs::create_dir(&journal).unwrap();
    fs::write(
        journal.join("intent.json"),
        br#"{"name":"../outside","had_old":false,"committed":false}"#,
    )
    .unwrap();
    assert!(reconcile(&dest, false).is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"keep");
    fs::remove_file(journal.join("intent.json")).unwrap();
    std::os::unix::fs::symlink(&outside, journal.join("new")).unwrap();
    assert!(reconcile(&dest, false).is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"keep");
}
