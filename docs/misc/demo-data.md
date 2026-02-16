# Demo Data for Screenshots

Synthetic test data for taking README/marketing screenshots with no real personal data.

## Generate

```bash
python3 docs/misc/generate-demo-data.py
```

Or re-run the inline script — data lands in `/tmp/find-demo`.

## Structure

```
/tmp/find-demo/
  projects/
    taskflow/          # fictional Rust task-management API
      README.md
      Cargo.toml
      config/default.toml
      src/main.rs, auth.rs, tasks.rs
      src/api/routes.rs, middleware.rs
      docs/architecture.md, deployment.md
    weather-cli/       # fictional Python weather CLI
      README.md
      pyproject.toml
      src/main.py, api.py
  notes/
    meeting-notes.md
    research-databases.md
    ideas.md
    onboarding-checklist.md
```

## Client config

Two sources for multi-source screenshots:

```toml
[[sources]]
name  = "projects"
paths = ["/tmp/find-demo/projects"]

[[sources]]
name  = "notes"
paths = ["/tmp/find-demo/notes"]
```

Or a single source:

```toml
[[sources]]
name  = "demo"
paths = ["/tmp/find-demo"]
```

## Good search terms for screenshots

| Search | What it shows |
|--------|---------------|
| `authentication` | Cross-file hits: `auth.rs`, `architecture.md`, `meeting-notes.md` |
| `rate limit` | Multi-type results: Rust, Markdown, meeting notes |
| `webhook` | Code + docs together |
| `password` | `auth.rs` + architecture docs |
| `deploy` | Deployment guide, systemd snippets, meeting notes |
| `cache` | Rust + Python + TOML — shows multi-language indexing |
