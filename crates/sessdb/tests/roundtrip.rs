use sessdb::{FORMAT_VERSION, Part, SessDb, StorageError, UploadParts, doc_key, kg_ids_key};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fresh() -> Result<(tempfile::TempDir, SessDb), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let db = sessdb::open(&tmp.path().join("rocksdb"))?;
    Ok((tmp, db))
}

fn parts<'a>(meta: &'a [u8], vectors: Option<&'a [u8]>) -> UploadParts<'a> {
    UploadParts {
        meta,
        doc: b"doc-bytes",
        paged_text: b"--- p1 ---\nbody\n",
        span_map: b"[]",
        findings: b"[]",
        vectors,
    }
}

#[test]
fn state_and_leaf_round_trip() -> TestResult {
    let (_tmp, db) = fresh()?;
    let key = kg_ids_key("s-1")?;
    assert_eq!(db.get_state(&key)?, None);
    db.put_state(&key, b"{\"entries\":[]}")?;
    assert_eq!(db.get_state(&key)?, Some(b"{\"entries\":[]}".to_vec()));
    db.put_leaf(b"cache-key", b"leaf")?;
    assert_eq!(db.get_leaf(b"cache-key")?, Some(b"leaf".to_vec()));
    assert_eq!(db.get_leaf(b"other")?, None);
    assert_eq!(FORMAT_VERSION, 1);
    Ok(())
}

#[test]
fn reopen_keeps_values() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("rocksdb");
    {
        let db = sessdb::open(&path)?;
        db.put_state(&kg_ids_key("s")?, b"v")?;
        db.flush()?;
    }
    let db = sessdb::open(&path)?;
    assert_eq!(db.get_state(&kg_ids_key("s")?)?, Some(b"v".to_vec()));
    Ok(())
}

#[test]
fn bad_key_parts_are_rejected() -> TestResult {
    let (_tmp, db) = fresh()?;
    assert!(matches!(
        kg_ids_key("a/b"),
        Err(StorageError::Key("session"))
    ));
    assert!(matches!(kg_ids_key(""), Err(StorageError::Key("session"))));
    assert!(matches!(
        doc_key("s", "f/x", Part::Meta),
        Err(StorageError::Key("file_id"))
    ));
    assert!(db.list_files("../x").is_err());
    assert!(db.delete_session("a/").is_err());
    Ok(())
}

#[test]
fn put_upload_writes_every_part_together() -> TestResult {
    let (_tmp, db) = fresh()?;
    db.put_upload("s1", "f1", &parts(b"meta-1", Some(b"vec")))?;
    for (part, want) in [
        (Part::Meta, b"meta-1".as_slice()),
        (Part::Model, b"doc-bytes"),
        (Part::Text, b"--- p1 ---\nbody\n"),
        (Part::Spans, b"[]"),
        (Part::Findings, b"[]"),
        (Part::Vectors, b"vec"),
    ] {
        assert_eq!(
            db.get_state(&doc_key("s1", "f1", part)?)?.as_deref(),
            Some(want)
        );
    }
    db.put_upload("s1", "f1", &parts(b"meta-2", None))?;
    assert_eq!(db.get_state(&doc_key("s1", "f1", Part::Vectors)?)?, None);
    assert_eq!(
        db.get_state(&doc_key("s1", "f1", Part::Meta)?)?.as_deref(),
        Some(b"meta-2".as_slice())
    );
    Ok(())
}

#[test]
fn failed_put_upload_leaves_nothing() -> TestResult {
    let (_tmp, db) = fresh()?;
    assert!(db.put_upload("s1", "bad/id", &parts(b"m", None)).is_err());
    assert!(db.put_upload("bad/s", "f1", &parts(b"m", None)).is_err());
    assert!(db.list_files("s1")?.is_empty());
    assert_eq!(db.get_state(&doc_key("s1", "f1", Part::Model)?)?, None);
    Ok(())
}

#[test]
fn list_files_reads_only_that_session_metas() -> TestResult {
    let (_tmp, db) = fresh()?;
    db.put_upload("s1", "fa", &parts(b"ma", None))?;
    db.put_upload("s1", "fb", &parts(b"mb", None))?;
    db.put_upload("s10", "fc", &parts(b"mc", None))?;
    db.put_upload("s", "fd", &parts(b"md", None))?;
    db.put_state(&kg_ids_key("s1")?, b"ids")?;
    let listed: Vec<(String, Vec<u8>)> = db
        .list_files("s1")?
        .into_iter()
        .map(|f| (f.file_id, f.meta))
        .collect();
    assert_eq!(
        listed,
        vec![
            ("fa".to_string(), b"ma".to_vec()),
            ("fb".to_string(), b"mb".to_vec())
        ]
    );
    Ok(())
}

#[test]
fn delete_session_clears_state_but_not_others_or_leaf_cache() -> TestResult {
    let (_tmp, db) = fresh()?;
    db.put_upload("s1", "fa", &parts(b"ma", None))?;
    db.put_state(&kg_ids_key("s1")?, b"ids")?;
    db.put_upload("s10", "fc", &parts(b"mc", None))?;
    db.put_upload("s0", "fe", &parts(b"me", None))?;
    db.put_leaf(b"k", b"leaf")?;
    db.delete_session("s1")?;
    assert!(db.list_files("s1")?.is_empty());
    assert_eq!(db.get_state(&kg_ids_key("s1")?)?, None);
    assert_eq!(db.list_files("s10")?.len(), 1);
    assert_eq!(db.list_files("s0")?.len(), 1);
    assert_eq!(db.get_leaf(b"k")?, Some(b"leaf".to_vec()));
    Ok(())
}
