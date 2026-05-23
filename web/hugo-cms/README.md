# Embedded Hugo CMS Scaffold

This scaffold is adapted from `geronimo-iia/llm-wiki-hugo-cms`.

It is embedded into the `llm-wiki` binary and written to `<wiki-repo>/site` by
`llm-wiki spaces create`, `llm-wiki spaces register`, and `llm-wiki web install`.
Hugo mounts the wiki content directory directly, so Markdown pages remain in the
Git-backed wiki and are not copied into the site.
