use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::frontmatter;

/// Default Hugo development server port.
pub const DEFAULT_WEB_PORT: u16 = 1313;
/// Default Hugo bind address. Localhost avoids exposing private notes.
pub const DEFAULT_WEB_BIND: &str = "127.0.0.1";

const HUGO_FILES: &[(&str, &str)] = &[
    ("hugo.toml", include_str!("../web/hugo-cms/hugo.toml")),
    ("Makefile", include_str!("../web/hugo-cms/Makefile")),
    (
        "assets/css/custom.css",
        include_str!("../web/hugo-cms/assets/css/custom.css"),
    ),
    (
        "layouts/index.html",
        include_str!("../web/hugo-cms/layouts/index.html"),
    ),
    (
        "layouts/partials/backlinks.html",
        include_str!("../web/hugo-cms/layouts/partials/backlinks.html"),
    ),
    (
        "layouts/partials/footer.html",
        include_str!("../web/hugo-cms/layouts/partials/footer.html"),
    ),
    (
        "layouts/partials/head.html",
        include_str!("../web/hugo-cms/layouts/partials/head.html"),
    ),
    (
        "layouts/partials/header.html",
        include_str!("../web/hugo-cms/layouts/partials/header.html"),
    ),
    (
        "layouts/partials/metadata.html",
        include_str!("../web/hugo-cms/layouts/partials/metadata.html"),
    ),
    (
        "layouts/partials/page-list-item.html",
        include_str!("../web/hugo-cms/layouts/partials/page-list-item.html"),
    ),
    (
        "layouts/partials/superseded.html",
        include_str!("../web/hugo-cms/layouts/partials/superseded.html"),
    ),
    (
        "layouts/shortcodes/mermaid.html",
        include_str!("../web/hugo-cms/layouts/shortcodes/mermaid.html"),
    ),
    (
        "layouts/_default/baseof.html",
        include_str!("../web/hugo-cms/layouts/_default/baseof.html"),
    ),
    (
        "layouts/_default/list.html",
        include_str!("../web/hugo-cms/layouts/_default/list.html"),
    ),
    (
        "layouts/_default/single.html",
        include_str!("../web/hugo-cms/layouts/_default/single.html"),
    ),
    (
        "layouts/_default/_markup/render-codeblock-mermaid.html",
        include_str!("../web/hugo-cms/layouts/_default/_markup/render-codeblock-mermaid.html"),
    ),
];

/// Outcome of writing the embedded Hugo site into a wiki repository.
#[derive(Debug, Serialize, Deserialize)]
pub struct WebInstallReport {
    /// Absolute path to the created or updated Hugo site directory.
    pub site_path: String,
    /// Number of scaffold files written.
    pub written: usize,
    /// Number of existing files left untouched because `force` was false.
    pub skipped: usize,
    /// Number of wiki content files synced into Hugo's generated content mirror.
    pub content_synced: usize,
}

/// Return the conventional Hugo site path for a wiki repository.
pub fn site_path(repo_root: &Path) -> PathBuf {
    repo_root.join("site")
}

/// True when the wiki repository already has a Hugo site config.
pub fn is_installed(repo_root: &Path) -> bool {
    site_path(repo_root).join("hugo.toml").exists()
}

/// Write the embedded Hugo CMS scaffold into `<repo>/site`.
pub fn install_hugo_site(
    repo_root: &Path,
    title: &str,
    wiki_root: &str,
    force: bool,
) -> Result<WebInstallReport> {
    let site = site_path(repo_root);
    std::fs::create_dir_all(&site)
        .with_context(|| format!("failed to create {}", site.display()))?;

    let wiki_root = normalize_mount_path(wiki_root)?;
    let mut written = 0;
    let mut skipped = 0;

    for (relative, content) in HUGO_FILES {
        let dest = site.join(relative);
        if dest.exists() && !force {
            skipped += 1;
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let content = if *relative == "hugo.toml" {
            render_hugo_toml(content, title, &wiki_root)
        } else {
            (*content).to_string()
        };
        std::fs::write(&dest, content)
            .with_context(|| format!("failed to write {}", dest.display()))?;
        written += 1;
    }

    let content_synced = sync_hugo_content(repo_root, &wiki_root)?;

    Ok(WebInstallReport {
        site_path: site.to_string_lossy().into_owned(),
        written,
        skipped,
        content_synced,
    })
}

/// Refresh `<repo>/site/content` from the wiki source directory.
///
/// Hugo treats `index.md` as a leaf bundle, so sibling Markdown files become
/// resources instead of pages. llm-wiki uses `index.md` for section pages, so
/// the web mirror converts only `type: section` index files to `_index.md`.
pub fn sync_hugo_content(repo_root: &Path, wiki_root: &str) -> Result<usize> {
    let wiki_root = normalize_mount_path(wiki_root)?;
    let source_root = repo_root.join(&wiki_root);
    let site = site_path(repo_root);
    let content_root = site.join("content");

    if !source_root.is_dir() {
        bail!(
            "wiki content directory not found: {}",
            source_root.display()
        );
    }

    if content_root.exists() {
        fs::remove_dir_all(&content_root)
            .with_context(|| format!("failed to remove {}", content_root.display()))?;
    }
    fs::create_dir_all(&content_root)
        .with_context(|| format!("failed to create {}", content_root.display()))?;

    let mut synced = 0usize;
    for entry in WalkDir::new(&source_root)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(&source_root)?;
        if should_skip_content_file(relative) {
            continue;
        }

        let mut dest = content_root.join(relative);
        if is_section_index(entry.path())? {
            dest = dest.with_file_name("_index.md");
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::copy(entry.path(), &dest).with_context(|| {
            format!(
                "failed to copy {} to {}",
                entry.path().display(),
                dest.display()
            )
        })?;
        synced += 1;
    }

    Ok(synced)
}

/// Return the installed Hugo version string, or `None` if Hugo is not on PATH.
pub fn hugo_version() -> Result<Option<String>> {
    match Command::new("hugo").arg("version").output() {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(Some(text))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("hugo version failed: {}", stderr.trim());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("failed to run hugo version"),
    }
}

/// Run `hugo server` in the installed site directory.
pub fn spawn_hugo_server(repo_root: &Path, bind: &str, port: u16, drafts: bool) -> Result<Child> {
    let site = site_path(repo_root);
    if !site.join("hugo.toml").exists() {
        bail!(
            "web site is not installed at {}. Run `llm-wiki web install` first.",
            site.display()
        );
    }

    let mut command = Command::new("hugo");
    command
        .arg("server")
        .arg("--bind")
        .arg(bind)
        .arg("--port")
        .arg(port.to_string())
        .arg("--source")
        .arg(&site)
        .stdin(Stdio::null());
    if drafts {
        command.arg("--buildDrafts");
    }

    command.spawn().with_context(
        || "failed to start Hugo. Install Hugo Extended >= 0.147 and ensure `hugo` is on PATH",
    )
}

/// Run a one-shot Hugo production build.
pub fn build_hugo_site(repo_root: &Path, minify: bool) -> Result<ExitStatus> {
    let site = site_path(repo_root);
    if !site.join("hugo.toml").exists() {
        bail!(
            "web site is not installed at {}. Run `llm-wiki web install` first.",
            site.display()
        );
    }

    let mut command = Command::new("hugo");
    command.arg("--source").arg(&site).arg("--gc");
    if minify {
        command.arg("--minify");
    }
    command.status().with_context(
        || "failed to run Hugo. Install Hugo Extended >= 0.147 and ensure `hugo` is on PATH",
    )
}

fn render_hugo_toml(template: &str, title: &str, wiki_root: &str) -> String {
    let _ = wiki_root;
    template
        .replace(
            "baseURL = \"https://example.github.io/my-wiki/\"",
            "baseURL = \"http://localhost:1313/\"",
        )
        .replace(
            "title = \"My Wiki\"",
            &format!("title = \"{}\"", toml_escape(title)),
        )
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn normalize_mount_path(wiki_root: &str) -> Result<String> {
    let path = Path::new(wiki_root);
    if wiki_root.trim().is_empty() || path.is_absolute() {
        bail!("wiki_root must be a non-empty relative path");
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        bail!("wiki_root must not contain `..` components");
    }
    Ok(wiki_root.replace('\\', "/"))
}

fn should_skip_content_file(relative: &Path) -> bool {
    let components: Vec<_> = relative
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect();
    if components
        .iter()
        .any(|c| matches!(c.as_ref(), "inbox" | "raw" | "schemas"))
    {
        return true;
    }
    if relative.file_name().and_then(|n| n.to_str()) == Some("LINT.md") {
        return true;
    }
    matches!(
        relative.extension().and_then(|e| e.to_str()),
        Some("json" | "txt")
    )
}

fn is_section_index(path: &Path) -> Result<bool> {
    if path.file_name().and_then(|n| n.to_str()) != Some("index.md") {
        return Ok(false);
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let page = frontmatter::parse(&content);
    Ok(page.page_type() == Some("section"))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn installs_hugo_scaffold_with_custom_mount() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("content/concepts")).unwrap();
        std::fs::write(
            tmp.path().join("content/concepts/moe.md"),
            "---\ntitle: MoE\ntype: concept\n---\n\nBody.\n",
        )
        .unwrap();

        let report = install_hugo_site(tmp.path(), "Brain MCP", "content", false).unwrap();

        assert_eq!(report.written, HUGO_FILES.len());
        assert_eq!(report.skipped, 0);
        assert_eq!(report.content_synced, 1);
        let config = std::fs::read_to_string(tmp.path().join("site/hugo.toml")).unwrap();
        assert!(config.contains("title = \"Brain MCP\""));
        assert!(config.contains("source = \"content\""));
        assert!(
            tmp.path()
                .join("site/layouts/_default/single.html")
                .exists()
        );
        assert!(tmp.path().join("site/content/concepts/moe.md").exists());
    }

    #[test]
    fn does_not_overwrite_without_force() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("wiki")).unwrap();
        install_hugo_site(tmp.path(), "First", "wiki", false).unwrap();
        let config_path = tmp.path().join("site/hugo.toml");
        std::fs::write(&config_path, "custom").unwrap();

        let report = install_hugo_site(tmp.path(), "Second", "wiki", false).unwrap();

        assert_eq!(report.written, 0);
        assert_eq!(report.skipped, HUGO_FILES.len());
        assert_eq!(std::fs::read_to_string(config_path).unwrap(), "custom");
    }

    #[test]
    fn sync_converts_section_index_to_hugo_branch_index() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("wiki/concepts")).unwrap();
        std::fs::write(
            tmp.path().join("wiki/concepts/index.md"),
            "---\ntitle: Concepts\ntype: section\n---\n\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("wiki/concepts/llm-wiki-pattern.md"),
            "---\ntitle: Pattern\ntype: concept\n---\n\nBody.\n",
        )
        .unwrap();

        let synced = sync_hugo_content(tmp.path(), "wiki").unwrap();

        assert_eq!(synced, 2);
        assert!(tmp.path().join("site/content/concepts/_index.md").exists());
        assert!(
            tmp.path()
                .join("site/content/concepts/llm-wiki-pattern.md")
                .exists()
        );
        assert!(!tmp.path().join("site/content/concepts/index.md").exists());
    }
}
