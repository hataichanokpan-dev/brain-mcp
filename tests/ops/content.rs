use super::helpers::setup_wiki;
use llm_wiki::engine::WikiEngine;
use llm_wiki::ops;

// ── Content ───────────────────────────────────────────────────────────────────

#[test]
fn content_read_page() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read();

    match ops::content_read(&engine, "concepts/moe", None, false, false).unwrap() {
        ops::ContentReadResult::Page(content) => {
            assert!(content.contains("Mixture of Experts"));
        }
        _ => panic!("expected Page"),
    }
}

#[test]
fn content_read_no_frontmatter() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read();

    match ops::content_read(&engine, "concepts/moe", None, true, false).unwrap() {
        ops::ContentReadResult::Page(content) => {
            assert!(!content.contains("title:"));
            assert!(content.contains("Mixture of Experts"));
        }
        _ => panic!("expected Page"),
    }
}

#[test]
fn content_read_bare_slug_falls_back_to_blueprint_sections() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read();

    match ops::content_read(&engine, "moe", None, true, false).unwrap() {
        ops::ContentReadResult::Page(content) => {
            assert!(content.contains("Mixture of Experts"));
        }
        _ => panic!("expected Page"),
    }
}

#[test]
fn content_write_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read();

    let body = "---\ntitle: \"New\"\ntype: page\n---\n\nHello.\n";
    let result = ops::content_write(&engine, "new-page", None, body).unwrap();
    assert_eq!(result.bytes_written, body.len());

    match ops::content_read(&engine, "new-page", None, false, false).unwrap() {
        ops::ContentReadResult::Page(content) => assert!(content.contains("Hello.")),
        _ => panic!("expected Page"),
    }
}

#[test]
fn content_commit_rehomes_bare_concept_slug() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read();
    let space = engine.space("test").unwrap();

    let root_file = space.wiki_root.join("thai-tts-voice-cloning.md");
    std::fs::write(
        &root_file,
        "---\ntitle: \"Thai TTS\"\ntype: concept\nstatus: active\n---\n\nBody.\n",
    )
    .unwrap();

    let hash = ops::content_commit(
        &engine,
        "test",
        &["thai-tts-voice-cloning".to_string()],
        false,
        Some("commit thai tts"),
    )
    .unwrap();

    assert!(!hash.is_empty());
    assert!(!root_file.exists());
    assert!(
        space
            .wiki_root
            .join("concepts/thai-tts-voice-cloning.md")
            .exists()
    );
}

#[test]
fn content_new_page() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read();

    let result = ops::content_new(
        &engine,
        "concepts/new-concept",
        None,
        false,
        false,
        None,
        None,
    )
    .unwrap();
    assert!(result.uri.starts_with("wiki://test/concepts/new-concept"));
    assert_eq!(result.slug, "concepts/new-concept");
    assert!(!result.bundle);
    assert!(result.path.exists());
    assert!(result.path.to_string_lossy().ends_with(".md"));
}

#[test]
fn content_new_section() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read();

    let result = ops::content_new(&engine, "topics", None, true, false, None, None).unwrap();
    assert!(result.uri.contains("topics"));
}

#[test]
fn content_new_bundle_result_has_path_and_wiki_root() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read();

    let result =
        ops::content_new(&engine, "concepts/bundled", None, false, true, None, None).unwrap();
    assert!(result.bundle);
    assert!(result.path.ends_with("index.md"));
    assert!(result.path.exists());
    assert!(result.wiki_root.is_dir());
}

#[test]
fn content_commit_all() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read();

    // Write a new file so there's something to commit
    ops::content_write(
        &engine,
        "scratch",
        None,
        "---\ntitle: \"Scratch\"\ntype: page\n---\n\ntemp\n",
    )
    .unwrap();

    let hash = ops::content_commit(&engine, "test", &[], true, Some("test commit")).unwrap();
    assert!(!hash.is_empty());
}
