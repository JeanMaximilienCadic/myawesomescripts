# /readme - Update the README

## Purpose
Intelligently update the project README by scanning the repository structure and refreshing relevant sections while preserving the overall template layout. Includes a reference to the CHANGELOG when one exists or should be created.

## Template
The README must follow this HTML/Markdown template structure. Adapt the content to match the actual project (not the Sakura example), but preserve the layout:

```html
<h1 align="center">
  <br>
  <img src="PROJECT_LOGO_URL" alt="PROJECT_NAME" width="200">
  <br>
  PROJECT_NAME
  <br>
</h1>

<p align="center">
  One-sentence description of what the project does.
</p>

<p align="center">
  <a href="#modules">Modules</a> •
  <a href="#code-structure">Code structure</a> •
  <a href="#code-design">Code design</a> •
  <a href="#installing-the-application">Installing the application</a> •
  <a href="#taskfile-commands">Taskfile commands</a> •
  <a href="#environments">Environments</a> •
  <a href="#running-the-application">Running the application</a> •
  <a href="#changelog">Changelog</a>
</p>

---
```

### Header rules

- `PROJECT_NAME` and `PROJECT_LOGO_URL` are placeholders to substitute, not literal text. Never emit a placeholder wrapped in angle brackets (`<project-logo-url>`): inside HTML the browser parses it as a tag, so the title vanishes and the image renders broken.
- **If the project has no logo, delete the `<img>` line and the `<br>` above it.** An `<img>` with an unresolved `src` renders as a broken-image icon. The heading must always carry the project name as text, so it stays readable with or without a logo.
- Keep a blank line between `</p>` and the first Markdown heading. Without it GitHub keeps parsing in HTML-block mode and the section below is swallowed.
- Only link nav entries whose sections actually exist (e.g. drop `Taskfile commands` when the project has no `Taskfile.yml`). A nav link to a missing heading is a dead anchor.

Followed by these sections:

### Sections to maintain

1. **Project description** - A short paragraph describing the project purpose.

2. **# Modules** - A table listing all top-level modules/directories with descriptions:
   ```
   | Component | Description |
   | ---- | --- |
   | **aws/** | AWS management tools (EC2, S3) |
   | **docker/** | Docker and container utilities |
   ...
   ```

3. **# Code structure** - A tree view of the repository structure showing all scripts and files. Auto-generate from the actual directory layout using `find` or `tree`. Exclude `.git/` and other irrelevant directories.

4. **# Code design** - Describe the design philosophy or patterns used. Preserve existing content if present; otherwise generate from codebase analysis.

5. **# Installing the application** - Prerequisites and setup instructions. Update if new dependencies are detected.

6. **# Taskfile commands** (or **# Makefile commands** if no Taskfile exists) - List available task/make targets with descriptions. Auto-discover from `Taskfile.yml` or `Makefile`.

7. **# Environments** - Environment variables and configuration. Scan scripts for `export`, `ENV`, or `.env` references.

8. **# Running the application** - Usage examples for the main scripts.

9. **# Changelog** - Add a section at the bottom referencing the CHANGELOG:
   ```markdown
   # Changelog

   See [CHANGELOG.md](CHANGELOG.md) for a detailed list of changes.
   ```
   - If `CHANGELOG.md` does not exist, create it using the `/changes-notes` agent format before referencing it.

## Instructions

1. **Scan the repository:**
   - Run `find . -not -path './.git/*' -not -name '.git' | sort` to get the full file tree.
   - Read existing `README.md` if present.
   - Read `Taskfile.yml` or `Makefile` if present for commands section.
   - Scan scripts for environment variable usage.

2. **Smart update strategy:**
   - Parse the existing README into sections (by `#` headers).
   - For each section, determine if it needs updating by comparing current repo state with section content.
   - Update only sections that are stale or missing.
   - Preserve any custom content the user has added that doesn't conflict with auto-generated content.

3. **CHANGELOG reference:**
   - Check if `CHANGELOG.md` exists.
   - If it does, ensure the Changelog section references it.
   - If it does not, inform the user it will be created, and generate an initial `CHANGELOG.md` with an `## [Unreleased]` section listing recent git commits grouped by type.

4. **Test commands found in the README:**
   - After generating or updating the README content, extract all shell/bash code blocks from the document.
   - Classify each command as **safe** or **unsafe**:
     - **Safe commands** (run these automatically):
       - `--help` or `--version` invocations
       - `--dry-run` variants of install/build commands
       - Read-only commands: `task --list`, `python -c "import package"`, `uv pip install --dry-run .`
       - Commands that only print output without side effects
     - **Unsafe commands** (do NOT run these):
       - Commands that write, install, delete, or modify files (`pip install`, `uv pip install .`, `docker build`, `rm`, `make install`, etc.)
       - Commands that start services or long-running processes
       - Commands that require interactive input
       - Commands that need network access to external services (excluding package index checks)
       - Commands that require elevated privileges (`sudo`)
   - For each safe command, run it and verify it exits successfully (exit code 0).
   - For unsafe commands, attempt a dry-run or validation equivalent when possible:
     - `pip install .` → run `uv pip install --dry-run .` or `pip install --dry-run .`
     - `docker build .` → verify `Dockerfile` exists and is valid syntax
     - `task <target>` → verify the target exists with `task --list`
     - Install commands → verify the package/dependency file referenced exists
   - Collect results into a summary table:
     ```
     | Command | Source Section | Status | Notes |
     | --- | --- | --- | --- |
     | `uv pip install .` | Installing | PASS (dry-run) | Resolved 2 packages |
     | `python demo.py` | Running | SKIP (side effects) | - |
     | `task --list` | Taskfile commands | PASS | 15 targets found |
     ```
   - If any command fails, flag the relevant README section as needing a fix and suggest a correction.
   - Present the test results to the user alongside the section update summary.

5. **Present changes:**
   - Show the user a summary of which sections were updated and why.
   - Show the command test results table from step 4.
   - Ask the user to confirm before writing.

6. **Write the updated README.md.**

## Rules
- Never delete user-written custom sections.
- Always preserve the template header layout.
- Adapt section names to the actual project (e.g., "Taskfile commands" vs "Makefile commands" based on what exists).
- Keep the README concise and scannable.
