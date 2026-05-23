use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use llm_wiki::engine::WikiEngine;
use llm_wiki::git;
use llm_wiki::mcp::{McpServer, tools};
use llm_wiki::spaces;
use serde_json::{Map, Value, json};

#[test]
fn tool_list_returns_30_tools() {
    let tools = tools::tool_list();
    assert_eq!(tools.len(), 30);
}

#[test]
fn tool_list_contains_expected_names() {
    let tools = tools::tool_list();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    let expected = [
        "wiki_spaces_create",
        "wiki_spaces_register",
        "wiki_spaces_list",
        "wiki_spaces_remove",
        "wiki_spaces_set_default",
        "wiki_config",
        "wiki_content_read",
        "wiki_content_write",
        "wiki_content_new",
        "wiki_content_commit",
        "wiki_search",
        "wiki_list",
        "wiki_ingest",
        "wiki_index_rebuild",
        "wiki_index_status",
        "wiki_graph",
        "wiki_history",
        "wiki_stats",
        "wiki_lint",
        "wiki_resolve",
        "wiki_suggest",
        "wiki_export",
        "profile_get",
        "semantic_search",
        "semantic_get",
        "procedural_find",
        "procedural_get",
        "graph_neighbors",
        "audit_history",
    ];
    for name in &expected {
        assert!(names.contains(name), "missing tool: {name}");
    }
}

#[test]
fn tool_list_no_removed_tools() {
    let tools = tools::tool_list();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    let removed = [
        "wiki_init",
        "wiki_read",
        "wiki_write",
        "wiki_new_page",
        "wiki_new_section",
        "wiki_commit",
        "wiki_index_check",
    ];
    for name in &removed {
        assert!(!names.contains(name), "tool should be removed: {name}");
    }
}

#[test]
fn tool_list_all_have_descriptions() {
    for tool in &tools::tool_list() {
        assert!(
            !tool.description.as_ref().is_none_or(|d| d.is_empty()),
            "tool {} has empty description",
            tool.name
        );
    }
}

#[test]
fn tool_list_all_have_object_schema() {
    for tool in &tools::tool_list() {
        let schema = &tool.input_schema;
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "tool {} schema is not an object",
            tool.name
        );
    }
}

#[test]
fn spaces_create_requires_path_and_name() {
    let tools = tools::tool_list();
    let tool = tools
        .iter()
        .find(|t| t.name == "wiki_spaces_create")
        .unwrap();
    let required = tool
        .input_schema
        .get("required")
        .unwrap()
        .as_array()
        .unwrap();
    let req: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(req.contains(&"path"));
    assert!(req.contains(&"name"));
}

#[test]
fn content_new_has_section_and_name_and_type_params() {
    let tools = tools::tool_list();
    let tool = tools.iter().find(|t| t.name == "wiki_content_new").unwrap();
    let props = tool
        .input_schema
        .get("properties")
        .unwrap()
        .as_object()
        .unwrap();
    assert!(props.contains_key("section"), "missing section param");
    assert!(props.contains_key("name"), "missing name param");
    assert!(props.contains_key("type"), "missing type param");
    assert!(props.contains_key("bundle"), "missing bundle param");
}

#[test]
fn search_has_type_param() {
    let tools = tools::tool_list();
    let tool = tools.iter().find(|t| t.name == "wiki_search").unwrap();
    let props = tool
        .input_schema
        .get("properties")
        .unwrap()
        .as_object()
        .unwrap();
    assert!(props.contains_key("type"), "missing type param");
}

#[test]
fn wiki_resolve_has_uri_required() {
    let tools = tools::tool_list();
    let tool = tools.iter().find(|t| t.name == "wiki_resolve").unwrap();
    let required = tool
        .input_schema
        .get("required")
        .unwrap()
        .as_array()
        .unwrap();
    let req: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(req.contains(&"uri"));
    assert!(!req.contains(&"wiki"), "wiki should be optional");
    let props = tool
        .input_schema
        .get("properties")
        .unwrap()
        .as_object()
        .unwrap();
    assert!(props.contains_key("uri"));
    assert!(props.contains_key("wiki"));
}

#[test]
fn graph_has_relation_param() {
    let tools = tools::tool_list();
    let tool = tools.iter().find(|t| t.name == "wiki_graph").unwrap();
    let props = tool
        .input_schema
        .get("properties")
        .unwrap()
        .as_object()
        .unwrap();
    assert!(props.contains_key("relation"), "missing relation param");
}

fn write_page(wiki_root: &Path, slug: &str, content: &str) {
    let path = wiki_root.join(format!("{slug}.md"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn setup_mcp_smoke_wiki(dir: &Path) -> (PathBuf, PathBuf) {
    let config_path = dir.join("state").join("config.toml");
    let repo_root = dir.join("test");
    spaces::create(&repo_root, "test", None, false, true, &config_path, None).unwrap();

    let wiki_root = repo_root.join("wiki");
    write_page(
        &wiki_root,
        "concepts/moe",
        "---\ntitle: \"MoE\"\ntype: concept\nstatus: active\ntags: [ml]\n---\n\nMixture of Experts links to [[concepts/transformer]].\n",
    );
    write_page(
        &wiki_root,
        "concepts/transformer",
        "---\ntitle: \"Transformer\"\ntype: concept\nstatus: active\n---\n\nAttention model links to [[concepts/moe]].\n",
    );
    write_page(
        &wiki_root,
        "profiles/rules",
        "---\ntitle: \"Rules\"\ntype: profile\nsection: rules\npriority: hard\nstatus: active\n---\n\nAlways verify MCP tool calls.\n",
    );
    write_page(
        &wiki_root,
        "procedures/rebuild-index",
        "---\ntitle: \"Rebuild Index\"\ntype: procedure\nstatus: verified\nverification: [\"cargo test\"]\n---\n\nRun index rebuild and verify search.\n",
    );
    write_page(
        &wiki_root,
        "decisions/adr-mcp",
        "---\ntitle: \"MCP Decision\"\ntype: decision\nstatus: active\nsummary: \"Use MCP smoke tests\"\n---\n\nThe MCP server should call every tool.\n",
    );
    write_page(
        &wiki_root,
        "inbox/to-ingest",
        "---\ntitle: \"To Ingest\"\ntype: doc\nstatus: active\n---\n\nA valid page for ingest dry-run.\n",
    );

    git::commit(&repo_root, "add smoke pages").unwrap();
    (config_path, repo_root)
}

fn existing_wiki_repo(path: &Path) {
    fs::create_dir_all(path.join("wiki")).unwrap();
    fs::write(path.join("wiki.toml"), "name = \"registered-space\"\n").unwrap();
    git::init_repo(path).unwrap();
    git::commit(path, "init registered space").unwrap();
}

fn args(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn mcp_tool_dispatch_smoke_calls_every_registered_tool() {
    let dir = tempfile::tempdir().unwrap();
    let (config_path, _repo_root) = setup_mcp_smoke_wiki(dir.path());
    let manager = Arc::new(WikiEngine::build(&config_path).unwrap());
    let server = McpServer::new(manager);

    let create_path = dir.path().join("created-space");
    let register_path = dir.path().join("registered-space");
    existing_wiki_repo(&register_path);
    let export_path = dir.path().join("mcp-export.json");

    let calls: Vec<(&str, Map<String, Value>)> = vec![
        (
            "wiki_spaces_create",
            args(json!({
                "path": create_path.to_string_lossy(),
                "name": "created-space",
                "set_default": false
            })),
        ),
        (
            "wiki_spaces_register",
            args(json!({
                "path": register_path.to_string_lossy(),
                "name": "registered-space"
            })),
        ),
        ("wiki_spaces_list", args(json!({}))),
        ("wiki_spaces_set_default", args(json!({"name": "test"}))),
        (
            "wiki_spaces_remove",
            args(json!({"name": "registered-space", "delete": false})),
        ),
        ("wiki_config", args(json!({"action": "list"}))),
        (
            "wiki_content_read",
            args(json!({"uri": "concepts/moe", "wiki": "test"})),
        ),
        (
            "wiki_content_write",
            args(json!({
                "uri": "scratch/mcp-written",
                "wiki": "test",
                "content": "---\ntitle: \"MCP Written\"\ntype: concept\nstatus: active\n---\n\nWritten through MCP smoke test.\n"
            })),
        ),
        (
            "wiki_content_new",
            args(json!({
                "uri": "scratch/new-page",
                "wiki": "test",
                "type": "concept",
                "name": "New Page"
            })),
        ),
        (
            "wiki_content_commit",
            args(json!({"wiki": "test", "message": "mcp smoke content changes"})),
        ),
        (
            "wiki_search",
            args(json!({"query": "Mixture", "wiki": "test", "top_k": 5})),
        ),
        (
            "wiki_list",
            args(json!({"wiki": "test", "type": "concept", "page_size": 10})),
        ),
        (
            "wiki_ingest",
            args(json!({"wiki": "test", "path": "inbox/to-ingest.md", "dry_run": true})),
        ),
        ("wiki_index_rebuild", args(json!({"wiki": "test"}))),
        ("wiki_index_status", args(json!({"wiki": "test"}))),
        (
            "wiki_graph",
            args(json!({"wiki": "test", "format": "llms", "root": "concepts/moe", "depth": 1})),
        ),
        (
            "wiki_history",
            args(json!({"wiki": "test", "slug": "concepts/moe", "limit": 5})),
        ),
        ("wiki_stats", args(json!({"wiki": "test"}))),
        ("wiki_lint", args(json!({"wiki": "test"}))),
        (
            "wiki_resolve",
            args(json!({"wiki": "test", "uri": "concepts/moe"})),
        ),
        (
            "wiki_suggest",
            args(json!({"wiki": "test", "slug": "concepts/transformer", "limit": 5})),
        ),
        (
            "wiki_schema",
            args(json!({"wiki": "test", "action": "list"})),
        ),
        (
            "wiki_export",
            args(json!({
                "wiki": "test",
                "format": "json",
                "path": export_path.to_string_lossy()
            })),
        ),
        (
            "profile_get",
            args(json!({"wiki": "test", "section": "rules"})),
        ),
        (
            "semantic_search",
            args(json!({"wiki": "test", "query": "decision", "type": "decision", "top_k": 5})),
        ),
        (
            "semantic_get",
            args(json!({"wiki": "test", "page_id": "decisions/adr-mcp", "with_backlinks": true})),
        ),
        (
            "procedural_find",
            args(json!({"wiki": "test", "intent": "rebuild index", "top_k": 5})),
        ),
        (
            "procedural_get",
            args(json!({"wiki": "test", "proc_id": "procedures/rebuild-index"})),
        ),
        (
            "graph_neighbors",
            args(json!({"wiki": "test", "page_id": "concepts/moe", "depth": 1})),
        ),
        (
            "audit_history",
            args(json!({"wiki": "test", "path": "concepts/moe", "limit": 5})),
        ),
    ];

    let registered_names: Vec<String> = tools::tool_list()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();
    let called_names: Vec<&str> = calls.iter().map(|(name, _)| *name).collect();

    for name in &registered_names {
        assert!(
            called_names.contains(&name.as_str()),
            "missing MCP smoke call for registered tool: {name}"
        );
    }
    assert_eq!(registered_names.len(), calls.len());

    for (name, call_args) in calls {
        let result = tools::call(&server, name, &call_args);
        assert!(!result.is_error, "MCP tool {name} returned an error");
    }
}
