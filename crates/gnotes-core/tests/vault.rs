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

#[test]
fn export_then_import_roundtrip() {
    let dir = tmp();
    let mut v = Vault::create(dir.path().join("v.gnotes"), "pw").unwrap();
    v.create_folder("projects/thesis").unwrap();
    let img = v.add_attachment("my pic (1).png", None, b"PNGDATA").unwrap();
    let pdf = v.add_attachment("paper.pdf", None, b"PDFDATA").unwrap();
    let a = v.create_note("projects/thesis", Some("Draft")).unwrap();
    v.save_note(&a.id, &format!("# Draft\n![p](attachment:{})\n[📎 paper](attachment:{})\n", img.id, pdf.id)).unwrap();
    let b = v.create_note("", Some("Root note")).unwrap();
    v.save_note(&b.id, &format!("again ![p](attachment:{})", img.id)).unwrap();

    let out = dir.path().join("export");
    let rep = v.export_to_dir(&out).unwrap();
    assert_eq!((rep.notes, rep.attachments), (2, 2));
    let draft = fs::read_to_string(out.join("projects/thesis/Draft.md")).unwrap();
    assert!(draft.contains("![p](../../attachments/my%20pic%20%281%29.png)"), "{draft}");
    assert!(draft.contains("(../../attachments/paper.pdf)"));
    let root = fs::read_to_string(out.join("Root note.md")).unwrap();
    assert_eq!(root, "again ![p](attachments/my%20pic%20%281%29.png)");
    assert_eq!(fs::read(out.join("attachments/my pic (1).png")).unwrap(), b"PNGDATA");
    assert!(v.export_to_dir(&out).is_err(), "must not write into an existing folder");

    // Re-import into a fresh vault: links become attachment: references again.
    let mut w = Vault::create(dir.path().join("w.gnotes"), "pw").unwrap();
    let rep = w.import_dir(&out, "", true).unwrap();
    assert_eq!((rep.folder.as_str(), rep.notes, rep.attachments), ("export", 2, 2));
    assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
    let t = w.tree().unwrap();
    assert_eq!(t.folders, ["export", "export/projects", "export/projects/thesis"]);
    let draft = t.notes.iter().find(|n| n.title == "Draft").unwrap();
    assert_eq!(draft.folder, "export/projects/thesis");
    let body = w.get_note(&draft.id).unwrap().body;
    let refs = gnotes_core::attachments::find_refs(&body);
    assert_eq!(refs.len(), 2, "{body}");
    let (meta, data) = w.attachment(refs[0].1).unwrap();
    assert_eq!((meta.name.as_str(), data.as_slice()), ("my pic (1).png", &b"PNGDATA"[..]));
    // Same file referenced from two notes is stored once.
    assert_eq!(w.attachments().unwrap().len(), 2);
    // Importing again goes into "export 2".
    assert_eq!(w.import_dir(&out, "", true).unwrap().folder, "export 2");
}

#[test]
fn import_obsidian_style_vault() {
    let dir = tmp();
    let src = dir.path().join("My Vault");
    fs::create_dir_all(src.join("assets")).unwrap();
    fs::create_dir_all(src.join("daily")).unwrap();
    fs::create_dir_all(src.join(".obsidian")).unwrap();
    fs::write(src.join(".obsidian/app.json"), "{}").unwrap();
    fs::write(src.join("assets/pic.png"), b"IMG").unwrap();
    fs::write(src.join("doc.pdf"), b"PDF").unwrap();
    fs::write(dir.path().join("secret.txt"), b"outside").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.path().join("secret.txt"), src.join("link.txt")).unwrap();
    fs::write(
        src.join("daily/2026-11-01.md"),
        "---\ntags: [x]\n---\n![[pic.png|300]]\n![[doc.pdf]]\n![[Other note]]\n[[Other note]]\n\
         ![rel](../assets/pic.png)\n[ext](https://example.com)\n![gone](nope.png)\n\
         [leak](link.txt)\n<img src=\"../assets/pic.png\" width=\"10\">\n```\n![[pic.png]]\n```\n",
    )
    .unwrap();
    fs::write(src.join("Other note.md"), "plain").unwrap();
    fs::write(src.join("bad:name?.md"), "x").unwrap();

    let mut v = Vault::create(dir.path().join("v.gnotes"), "pw").unwrap();
    let rep = v.import_dir(&src, "", true).unwrap();
    assert_eq!((rep.folder.as_str(), rep.notes, rep.attachments), ("My Vault", 3, 2));
    assert_eq!(rep.warnings.len(), 1, "{:?}", rep.warnings);
    assert!(rep.warnings[0].contains("nope.png"));

    let t = v.tree().unwrap();
    assert!(t.notes.iter().any(|n| n.title == "bad-name-" && n.folder == "My Vault"));
    let daily = t.notes.iter().find(|n| n.folder == "My Vault/daily").unwrap();
    let body = v.get_note(&daily.id).unwrap().body;
    let pic = v.attachments().unwrap().into_iter().find(|a| a.name == "pic.png").unwrap();
    let doc = v.attachments().unwrap().into_iter().find(|a| a.name == "doc.pdf").unwrap();
    assert!(body.contains(&format!("![pic.png](attachment:{})", pic.id)), "{body}");
    assert!(body.contains(&format!("[📎 doc.pdf](attachment:{})", doc.id)));
    assert!(body.contains("![[Other note]]\n[[Other note]]"), "note embeds/links untouched");
    assert!(body.contains(&format!("![rel](attachment:{})", pic.id)));
    assert!(body.contains("[ext](https://example.com)"));
    assert!(body.contains("![gone](nope.png)"));
    assert!(body.contains("[leak](link.txt)"), "symlink outside the folder must not be imported");
    assert!(body.contains(&format!("<img src=\"attachment:{}\" width=\"10\">", pic.id)));
    assert!(body.contains("```\n![[pic.png]]\n```"), "code blocks untouched");
    assert!(body.starts_with("---\ntags: [x]\n---\n"));
}

#[test]
fn link_shared_image_folder() {
    let dir = tmp();
    let img = dir.path().join("img");
    fs::create_dir_all(img.join("2025")).unwrap();
    fs::write(img.join("Photo.PNG"), b"P1").unwrap();
    fs::write(img.join("2025/chart.png"), b"C-2025").unwrap();
    fs::create_dir_all(img.join("old")).unwrap();
    fs::write(img.join("old/chart.png"), b"C-OLD").unwrap();
    fs::write(img.join("unused.png"), b"U").unwrap();

    let mut v = Vault::create(dir.path().join("v.gnotes"), "pw").unwrap();
    v.create_folder("work").unwrap();
    let a = v.create_note("work", Some("A")).unwrap();
    let b = v.create_note("", Some("B")).unwrap();
    let c = v.create_note("", Some("C")).unwrap();
    v.save_note(&a.id, "![](../img/photo.png)\n![[chart.png]]\n![x](img/2025/chart.png)\n![m](img/missing.png)").unwrap();
    v.save_note(&b.id, "<img src=\"img/Photo.PNG\">\n![[Photo.PNG|200]]").unwrap();
    v.save_note(&c.id, "no images here").unwrap();
    let before = v.get_note(&a.id).unwrap().updated;

    let rep = v.link_files_from_dir(&img).unwrap();
    assert_eq!((rep.notes, rep.links), (2, 5), "{rep:?}");
    // photo + one chart (both chart links resolve to the same best match); unused is skipped.
    let atts = v.attachments().unwrap();
    assert!(atts.iter().all(|m| m.name != "unused.png"));
    assert_eq!(rep.attachments, atts.len());
    assert_eq!(rep.warnings, ["work/A: missing img/missing.png"]);

    let body_a = v.get_note(&a.id).unwrap();
    assert_eq!(body_a.updated, before, "repair must not bump the modified time");
    let refs = gnotes_core::attachments::find_refs(&body_a.body);
    assert_eq!(refs.len(), 3, "{}", body_a.body);
    // The explicit 2025 path picked the 2025 file, not old/chart.png.
    assert_eq!(v.attachment(refs[2].1).unwrap().1, b"C-2025");
    assert!(body_a.body.ends_with("![m](img/missing.png)"));

    // The shared photo is stored once for both notes.
    let body_b = v.get_note(&b.id).unwrap().body;
    let photo_a = refs[0].1.to_string();
    assert!(gnotes_core::attachments::find_refs(&body_b).iter().all(|(_, id)| *id == photo_a), "{body_b}");

    // Running it again changes nothing.
    let again = v.link_files_from_dir(&img).unwrap();
    assert_eq!((again.notes, again.links, again.attachments), (0, 0, 0));
}

#[test]
fn import_into_chosen_folder() {
    let dir = tmp();
    let src = dir.path().join("Old Vault");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("Plan.md"), "new plan").unwrap();
    fs::write(src.join("sub/Deep.md"), "deep").unwrap();

    let mut v = Vault::create(dir.path().join("v.gnotes"), "pw").unwrap();
    v.create_folder("Work/Projects").unwrap();
    let mine = v.create_note("Work/Projects", Some("Plan")).unwrap();
    v.save_note(&mine.id, "my plan").unwrap();

    // Into a nested folder, wrapped in a new "Old Vault" folder.
    let r = v.import_dir(&src, "Work/Projects", true).unwrap();
    assert_eq!((r.folder.as_str(), r.notes), ("Work/Projects/Old Vault", 2));
    // Into a folder that doesn't exist yet: created with its parents.
    let r = v.import_dir(&src, "/Archive/2025/", true).unwrap();
    assert_eq!(r.folder, "Archive/2025/Old Vault");
    // Merged directly: the clashing title gets a number, mine is untouched.
    let r = v.import_dir(&src, "Work/Projects", false).unwrap();
    assert_eq!((r.folder.as_str(), r.notes), ("Work/Projects", 2));
    let t = v.tree().unwrap();
    for f in ["Archive", "Archive/2025", "Archive/2025/Old Vault", "Work/Projects/Old Vault/sub", "Work/Projects/sub"] {
        assert!(t.folders.iter().any(|x| x == f), "missing folder {f}: {:?}", t.folders);
    }
    let in_projects: Vec<_> = t.notes.iter().filter(|n| n.folder == "Work/Projects").map(|n| n.title.as_str()).collect();
    assert!(in_projects.contains(&"Plan") && in_projects.contains(&"Plan 2"), "{in_projects:?}");
    assert_eq!(v.get_note(&mine.id).unwrap().body, "my plan");
    // Invalid destination is rejected before anything is written.
    assert!(v.import_dir(&src, "a/../b", true).is_err());
}

#[test]
fn search_matches_all_terms_case_insensitively() {
    let dir = tempfile::tempdir().unwrap();
    let mut v = Vault::create(dir.path().join("v.gnotes"), "pw").unwrap();
    v.create_folder("work").unwrap();
    let a = v.create_note("work", Some("Rust notes")).unwrap();
    v.save_note(&a.id, "intro\nThe Borrow checker is strict\nnothing here\nborrow again").unwrap();
    let b = v.create_note("", Some("Cooking")).unwrap();
    v.save_note(&b.id, "borrow a cup of sugar").unwrap();
    let c = v.create_note("", Some("Other")).unwrap();
    v.save_note(&c.id, "unrelated").unwrap();

    let r = v.search("BORROW").unwrap();
    assert_eq!(r.total, 2);
    assert_eq!(r.hits[0].id, a.id, "more matching lines ranks first");
    assert_eq!(r.hits[0].line_count, 2);
    assert_eq!(r.hits[0].lines[0].line, 1);
    assert_eq!(r.hits[0].lines[0].text, "The Borrow checker is strict");

    // Every term must match, in the title/folder or body.
    let r = v.search("borrow rust").unwrap();
    assert_eq!(r.hits.len(), 1);
    assert!(r.hits[0].title_match);
    assert_eq!(v.search("work sugar").unwrap().total, 0);
    assert_eq!(v.search("\"cup of sugar\"").unwrap().hits[0].id, b.id);
    assert_eq!(v.search("\"sugar cup\"").unwrap().total, 0);

    // Title matches rank above body-only matches.
    let r = v.search("other").unwrap();
    assert_eq!(r.hits[0].id, c.id);
    assert!(r.hits[0].lines.is_empty());
    assert_eq!(v.search("   ").unwrap().total, 0);
}

#[test]
fn tags_and_tag_search() {
    let dir = tempfile::tempdir().unwrap();
    let mut v = Vault::create(dir.path().join("v.gnotes"), "pw").unwrap();
    let a = v.create_note("", Some("Papers")).unwrap();
    v.save_note(&a.id, "---\ntags: [ai]\n---\nreading list\n#ai/llm transformers").unwrap();
    let b = v.create_note("", Some("Lab")).unwrap();
    v.save_note(&b.id, "#ai #Work\n```\n#notatag\n```").unwrap();
    let c = v.create_note("", Some("Plain")).unwrap();
    v.save_note(&c.id, "# Heading only, issue #42").unwrap();

    let tags: Vec<(String, usize)> = v.tags().unwrap().into_iter().map(|t| (t.tag, t.count)).collect();
    assert_eq!(tags, [("ai".into(), 2), ("ai/llm".into(), 1), ("work".into(), 1)]);

    // A tag matches itself and nested tags; both spellings work.
    let ids = |q: &str| {
        let mut ids: Vec<String> = v.search(q).unwrap().hits.into_iter().map(|h| h.id).collect();
        ids.sort();
        ids
    };
    let mut ab = vec![a.id.clone(), b.id.clone()];
    ab.sort();
    assert_eq!(ids("#ai"), ab);
    assert_eq!(ids("tag:AI"), ab);
    assert_eq!(ids("#ai/llm"), [a.id.clone()]);
    assert_eq!(ids("#ai #work"), [b.id.clone()]);
    assert_eq!(ids("#ai transformers"), [a.id.clone()]);
    assert!(ids("#notatag").is_empty());
    assert!(ids("#ai/l").is_empty());

    let r = v.search("#ai/llm").unwrap();
    assert_eq!(r.tags, ["ai/llm"]);
    assert_eq!(r.terms, ["#ai/llm"]);
    assert_eq!(r.hits[0].lines[0].line, 4);
    // "#42" is not a tag, so it is searched as text.
    assert_eq!(ids("#42"), [c.id.clone()]);
}

#[test]
fn ui_state_is_stored_in_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v.gnotes");
    {
        let mut v = Vault::create(&path, "pw").unwrap();
        assert_eq!(v.ui_get("ui.tabs").unwrap(), None);
        v.ui_set("ui.tabs", r#"{"ids":["a"]}"#).unwrap();
        v.ui_set("ui.tabs", r#"{"ids":["a","b"]}"#).unwrap();
        assert!(v.ui_set("salt", "x").is_err(), "only ui.* keys");
        assert!(v.ui_get("ui.").is_err());
        assert!(v.ui_set("ui.big", &"x".repeat(70_000)).is_err());
    }
    let v = Vault::open(&path, "pw").unwrap();
    assert_eq!(v.ui_get("ui.tabs").unwrap().as_deref(), Some(r#"{"ids":["a","b"]}"#));
}
