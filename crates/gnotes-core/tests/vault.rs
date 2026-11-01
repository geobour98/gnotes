use gnotes_core::{Error, Vault};
use std::fs;

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn create_reopen_and_wrong_password() {
    let dir = tmp();
    let path = dir.path().join("my.gnotes");
    {
        let mut v = Vault::create(&path, "correct horse").unwrap();
        let n = v.create_note("", Some("Hello")).unwrap();
        v.save_note(&n.id, "# secret body marker").unwrap();
    }
    assert!(matches!(Vault::create(&path, "x"), Err(Error::VaultExists)));
    assert!(matches!(Vault::open(&path, "wrong"), Err(Error::WrongPassword)));

    let v = Vault::open(&path, "correct horse").unwrap();
    let tree = v.tree().unwrap();
    assert_eq!(tree.notes.len(), 1);
    assert_eq!(v.get_note(&tree.notes[0].id).unwrap().body, "# secret body marker");
}

#[test]
fn file_contains_no_plaintext() {
    let dir = tmp();
    let path = dir.path().join("v.gnotes");
    let mut v = Vault::create(&path, "pw").unwrap();
    v.create_folder("TopSecretFolder").unwrap();
    let n = v.create_note("TopSecretFolder", Some("PlainTitle")).unwrap();
    v.save_note(&n.id, "PLAINTEXT_MARKER_12345").unwrap();
    drop(v);
    let bytes = fs::read(&path).unwrap();
    for needle in ["PLAINTEXT_MARKER", "TopSecretFolder", "PlainTitle", "SQLite format"] {
        assert!(!bytes.windows(needle.len()).any(|w| w == needle.as_bytes()), "found {needle}");
    }
}

#[test]
fn missing_and_garbage_files() {
    let dir = tmp();
    assert!(matches!(Vault::open(dir.path().join("nope"), "pw"), Err(Error::VaultNotFound)));
    let junk = dir.path().join("junk");
    fs::write(&junk, vec![7u8; 8192]).unwrap();
    assert!(matches!(Vault::open(&junk, "pw"), Err(Error::WrongPassword)));
}

#[test]
fn change_password() {
    let dir = tmp();
    let path = dir.path().join("v.gnotes");
    let mut v = Vault::create(&path, "old").unwrap();
    v.create_note("", Some("n")).unwrap();
    v.change_password("new").unwrap();
    drop(v);
    assert!(matches!(Vault::open(&path, "old"), Err(Error::WrongPassword)));
    assert_eq!(Vault::open(&path, "new").unwrap().tree().unwrap().notes.len(), 1);
}

#[test]
fn backup_is_encrypted_and_opens() {
    let dir = tmp();
    let path = dir.path().join("v.gnotes");
    let backup = dir.path().join("backups/b1.gnotes");
    let mut v = Vault::create(&path, "pw").unwrap();
    let n = v.create_note("", Some("n")).unwrap();
    v.save_note(&n.id, "backed up").unwrap();
    v.backup_to(&backup).unwrap();
    assert!(v.backup_to(&backup).is_err(), "must not overwrite");
    drop(v);
    let b = Vault::open(&backup, "pw").unwrap();
    assert_eq!(b.get_note(&n.id).unwrap().body, "backed up");
    assert!(matches!(Vault::open(&backup, "nope"), Err(Error::WrongPassword)));
}

#[test]
fn folders_and_notes() {
    let dir = tmp();
    let mut v = Vault::create(dir.path().join("v.gnotes"), "pw").unwrap();

    v.create_folder("projects/thesis").unwrap();
    assert_eq!(v.tree().unwrap().folders, ["projects", "projects/thesis"]);
    assert!(matches!(v.create_folder("projects"), Err(Error::AlreadyExists(_))));
    assert!(v.create_folder("bad/../x").is_err());

    let a = v.create_note("projects/thesis", None).unwrap();
    let b = v.create_note("projects/thesis", None).unwrap();
    assert_eq!((a.title.as_str(), b.title.as_str()), ("Untitled", "Untitled 2"));
    assert!(matches!(v.move_note(&b.id, "projects/thesis", "Untitled"), Err(Error::AlreadyExists(_))));
    v.move_note(&b.id, "projects", "Ideas").unwrap();
    assert!(v.create_note("does/not/exist", None).is_err());

    // Rename moves subfolders and notes; prefix "projects" must not catch "projectsX".
    v.create_folder("projectsX").unwrap();
    let x = v.create_note("projectsX", Some("keep")).unwrap();
    v.rename_folder("projects", "archive/2026").unwrap();
    let t = v.tree().unwrap();
    assert_eq!(t.folders, ["archive", "archive/2026", "archive/2026/thesis", "projectsX"]);
    assert_eq!(v.get_note(&a.id).unwrap().folder, "archive/2026/thesis");
    assert_eq!(v.get_note(&b.id).unwrap().folder, "archive/2026");
    assert_eq!(v.get_note(&x.id).unwrap().folder, "projectsX");
    assert!(v.rename_folder("archive", "archive/inner").is_err());

    assert_eq!(v.delete_folder("archive/2026").unwrap(), 2);
    let t = v.tree().unwrap();
    assert_eq!(t.folders, ["archive", "projectsX"]);
    assert_eq!(t.notes.len(), 1);

    v.delete_note(&x.id).unwrap();
    assert!(matches!(v.get_note(&x.id), Err(Error::NotFound(_))));
}

#[test]
fn attachments_roundtrip_and_prune() {
    let dir = tmp();
    let path = dir.path().join("v.gnotes");
    let mut v = Vault::create(&path, "pw").unwrap();
    let png = b"\x89PNG\r\n\x1a\nATTACHMENT_PLAINTEXT_MARKER";
    let a = v.add_attachment("../shot.png", None, png).unwrap();
    assert_eq!((a.name.as_str(), a.mime.as_str(), a.size), ("shot.png", "image/png", png.len() as i64));
    let unused = v.add_attachment("x.pdf", None, b"pdf").unwrap();

    let n = v.create_note("", Some("n")).unwrap();
    v.save_note(&n.id, &format!("![s](attachment:{})", a.id)).unwrap();
    let (meta, data) = v.attachment(&a.id).unwrap();
    assert_eq!((meta, data.as_slice()), (a.clone(), &png[..]));
    drop(v);

    // Encrypted at rest.
    let bytes = std::fs::read(&path).unwrap();
    assert!(!bytes.windows(12).any(|w| w == b"ATTACHMENT_P"));

    // Reopening prunes the unreferenced one only.
    let v = Vault::open(&path, "pw").unwrap();
    assert!(v.attachment_meta(&a.id).is_ok());
    assert!(matches!(v.attachment_meta(&unused.id), Err(Error::NotFound(_))));
    assert_eq!(v.attachments().unwrap().len(), 1);
}

#[test]
fn attachment_size_limit() {
    let dir = tmp();
    let mut v = Vault::create(dir.path().join("v.gnotes"), "pw").unwrap();
    let big = vec![0u8; gnotes_core::attachments::MAX_SIZE + 1];
    assert!(matches!(v.add_attachment("big.bin", None, &big), Err(Error::TooLarge(_))));
}
