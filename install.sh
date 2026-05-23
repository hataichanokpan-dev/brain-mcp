#!/bin/bash
set -euo pipefail

REPO="${LLM_WIKI_RELEASE_REPO:-hataichanokpan-dev/brain-mcp}"
BINARY="llm-wiki"
INSTALL_DIR="${LLM_WIKI_INSTALL_DIR:-/usr/local/bin}"
INSTALL_HUGO="${LLM_WIKI_INSTALL_HUGO:-1}"
HUGO_REPO="gohugoio/hugo"
HUGO_MIN_VERSION="0.147.0"
GITHUB_TOKEN="${GITHUB_TOKEN:-${GH_TOKEN:-}}"

# ── Colors ─────────────────────────────────────────────────────────────────────

red() { printf "\033[31m%s\033[0m\n" "$1"; }
green() { printf "\033[32m%s\033[0m\n" "$1"; }
yellow() { printf "\033[33m%s\033[0m\n" "$1"; }
dim() { printf "\033[2m%s\033[0m\n" "$1"; }

github_curl() {
    if [ -n "$GITHUB_TOKEN" ]; then
        curl -fsSL \
            -H "Authorization: Bearer ${GITHUB_TOKEN}" \
            -H "Accept: application/vnd.github+json" \
            "$@"
    else
        curl -fsSL "$@"
    fi
}

# ── Prerequisites ──────────────────────────────────────────────────────────────

check_prereqs() {
    if ! command -v git &>/dev/null; then
        red "error: git is required but not installed"
        echo "Install git: https://git-scm.com/downloads"
        exit 1
    fi

    if ! command -v curl &>/dev/null; then
        red "error: curl is required but not installed"
        exit 1
    fi
}

# ── Platform detection ─────────────────────────────────────────────────────────

detect_platform() {
    local os arch

    case "$(uname -s)" in
        Linux*)  os="unknown-linux-gnu" ;;
        Darwin*) os="apple-darwin" ;;
        *)
            red "error: unsupported OS: $(uname -s)"
            echo "Build from source instead: git clone https://github.com/hataichanokpan-dev/brain-mcp && cd brain-mcp && cargo install --path ."
            exit 1
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)
            red "error: unsupported architecture: $(uname -m)"
            echo "Build from source instead: git clone https://github.com/hataichanokpan-dev/brain-mcp && cd brain-mcp && cargo install --path ."
            exit 1
            ;;
    esac

    TARGET="${arch}-${os}"
}

# ── Version ────────────────────────────────────────────────────────────────────

get_latest_version() {
    local url response
    url="https://api.github.com/repos/${REPO}/releases/latest"
    if ! response=$(github_curl "$url"); then
        VERSION=""
        return 1
    fi
    VERSION=$(printf "%s" "$response" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/')
    if [ -z "$VERSION" ]; then
        return 1
    fi
}

# ── Download and install ───────────────────────────────────────────────────────

install() {
    local url="https://github.com/${REPO}/releases/download/v${VERSION}/${TARGET}.tar.gz"
    local tmpdir
    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    echo "Installing ${BINARY} v${VERSION} (${TARGET})"
    dim "  downloading ${url}"

    github_curl "$url" -o "${tmpdir}/archive.tar.gz"
    tar xzf "${tmpdir}/archive.tar.gz" -C "$tmpdir"

    if [ ! -f "${tmpdir}/${BINARY}" ]; then
        red "error: binary not found in archive"
        exit 1
    fi

    chmod +x "${tmpdir}/${BINARY}"

    install_executable "${tmpdir}/${BINARY}" "${BINARY}"
}

install_from_source() {
    local tmpdir

    if ! command -v cargo &>/dev/null; then
        red "error: no GitHub release was found for ${REPO}, and cargo is not installed"
        echo "Create a GitHub release, or install Rust first: https://www.rust-lang.org/tools/install"
        exit 1
    fi

    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    yellow "warning: no GitHub release found for ${REPO}; building from source"
    dim "  cloning https://github.com/${REPO}.git"
    git clone --depth 1 "https://github.com/${REPO}.git" "${tmpdir}/src"

    dim "  building release binary"
    (cd "${tmpdir}/src" && cargo build --release --locked)

    if [ ! -f "${tmpdir}/src/target/release/${BINARY}" ]; then
        red "error: source build finished but binary was not found"
        exit 1
    fi

    install_executable "${tmpdir}/src/target/release/${BINARY}" "${BINARY}"
    VERSION="source"
}

install_executable() {
    local src="$1"
    local name="$2"

    if [ ! -d "$INSTALL_DIR" ]; then
        if mkdir -p "$INSTALL_DIR" 2>/dev/null; then
            :
        else
            dim "  creating ${INSTALL_DIR} (requires sudo)"
            sudo mkdir -p "$INSTALL_DIR"
        fi
    fi

    if [ -w "$INSTALL_DIR" ]; then
        mv "$src" "${INSTALL_DIR}/${name}"
    else
        dim "  installing to ${INSTALL_DIR} (requires sudo)"
        sudo mv "$src" "${INSTALL_DIR}/${name}"
    fi
}

# ── Web UI dependency ──────────────────────────────────────────────────────────

hugo_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "amd64" ;;
        aarch64|arm64) echo "arm64" ;;
        *)
            return 1
            ;;
    esac
}

latest_hugo_version() {
    local url="https://api.github.com/repos/${HUGO_REPO}/releases/latest"
    github_curl "$url" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/'
}

install_hugo_linux() {
    local version arch asset url tmpdir
    version="${LLM_WIKI_HUGO_VERSION:-}"
    if [ -z "$version" ]; then
        version="$(latest_hugo_version || true)"
    fi
    arch="$(hugo_arch || true)"
    if [ -z "$version" ] || [ -z "$arch" ]; then
        yellow "warning: could not determine Hugo Extended release for this platform"
        return 0
    fi

    asset="hugo_extended_${version}_linux-${arch}.tar.gz"
    url="https://github.com/${HUGO_REPO}/releases/download/v${version}/${asset}"
    tmpdir=$(mktemp -d)

    echo "Installing Hugo Extended v${version} (${arch})"
    dim "  downloading ${url}"

    if ! github_curl "$url" -o "${tmpdir}/hugo.tar.gz"; then
        rm -rf "$tmpdir"
        yellow "warning: Hugo Extended download failed"
        yellow "         install Hugo Extended >= ${HUGO_MIN_VERSION} manually: https://gohugo.io/installation/"
        return 0
    fi
    if ! tar xzf "${tmpdir}/hugo.tar.gz" -C "$tmpdir"; then
        rm -rf "$tmpdir"
        yellow "warning: failed to extract Hugo Extended archive"
        return 0
    fi

    if [ ! -f "${tmpdir}/hugo" ]; then
        rm -rf "$tmpdir"
        yellow "warning: Hugo binary not found in downloaded archive"
        return 0
    fi

    chmod +x "${tmpdir}/hugo"
    install_executable "${tmpdir}/hugo" "hugo"
    rm -rf "$tmpdir"
}

install_hugo_macos() {
    if command -v brew &>/dev/null; then
        echo "Installing Hugo via Homebrew"
        if ! brew install hugo; then
            yellow "warning: Homebrew could not install Hugo"
            yellow "         install Hugo Extended >= ${HUGO_MIN_VERSION}: https://gohugo.io/installation/"
        fi
    else
        yellow "warning: Homebrew not found; skipping Hugo install"
        yellow "         install Hugo Extended >= ${HUGO_MIN_VERSION}: https://gohugo.io/installation/"
    fi
}

install_hugo() {
    if [ "$INSTALL_HUGO" = "0" ]; then
        dim "  skipping Hugo install (LLM_WIKI_INSTALL_HUGO=0)"
        return 0
    fi

    if command -v hugo &>/dev/null; then
        green "✓ Hugo already installed"
        dim "  $(hugo version)"
        return 0
    fi

    case "$(uname -s)" in
        Linux*)  install_hugo_linux ;;
        Darwin*) install_hugo_macos ;;
        *)
            yellow "warning: unsupported OS for automatic Hugo install"
            ;;
    esac
}

# ── Verify ─────────────────────────────────────────────────────────────────────

verify() {
    if command -v "$BINARY" &>/dev/null; then
        green "✓ ${BINARY} v${VERSION} installed to ${INSTALL_DIR}/${BINARY}"
        dim "  $($BINARY --version)"
    else
        echo ""
        echo "${BINARY} installed to ${INSTALL_DIR}/${BINARY}"
        echo "but it's not on your PATH. Add this to your shell profile:"
        echo ""
        echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi

    if command -v hugo &>/dev/null; then
        green "✓ Hugo web UI dependency installed"
        dim "  $(hugo version)"
        dim "  New wikis get a site/ scaffold automatically. Run: llm-wiki web serve --wiki <name>"
    else
        yellow "warning: Hugo is not on PATH"
        yellow "         Web scaffolds are still installed by llm-wiki, but preview/build need Hugo Extended >= ${HUGO_MIN_VERSION}"
    fi
}

# ── Main ───────────────────────────────────────────────────────────────────────

main() {
    check_prereqs
    detect_platform
    if get_latest_version; then
        install
    else
        install_from_source
    fi
    install_hugo
    verify
}

main
