def _logs_dir(wiki_env):
    return wiki_env.config.parent / "logs"


def _seed_log(wiki_env):
    logs = _logs_dir(wiki_env)
    logs.mkdir(parents=True, exist_ok=True)
    (logs / "2000-01-01.log").write_text(
        "line1\nline2\nline3\nline4\nline5\n", encoding="utf-8"
    )


def test_logs_list_returns_files(wiki_env):
    _seed_log(wiki_env)
    result = wiki_env.run("logs", "list")
    assert "2000-01-01" in result.stdout


def test_logs_tail_returns_output(wiki_env):
    _seed_log(wiki_env)
    result = wiki_env.run("logs", "tail")
    assert "line" in result.stdout


def test_logs_tail_n_lines(wiki_env):
    _seed_log(wiki_env)
    result = wiki_env.run("logs", "tail", "--lines", "3")
    lines = [line for line in result.stdout.strip().splitlines() if line]
    assert len(lines) == 3
    assert "line3" in result.stdout


def test_logs_clear(wiki_env):
    _seed_log(wiki_env)
    result = wiki_env.run("logs", "clear")
    assert "removed 1" in result.stdout


def test_logs_list_empty_after_clear(wiki_env):
    _seed_log(wiki_env)
    wiki_env.run("logs", "clear")
    result = wiki_env.run("logs", "list")
    assert "no log" in result.stdout.lower()
