# /readme - Update the README

## Purpose
Intelligently update the project README by scanning the repository structure and refreshing relevant sections while preserving the overall template layout. Includes a reference to the CHANGELOG when one exists or should be created.

## Template
The README must follow this HTML/Markdown template structure. Adapt the content to match the actual project (not the Sakura example), but preserve the layout:

```html
<h1 align="center">
  <br>
  <img src="<project-logo-url-if-available>">
</h1>
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
```

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

4. **Present changes:**
   - Show the user a summary of which sections were updated and why.
   - Ask the user to confirm before writing.

5. **Write the updated README.md.**

## Rules
- Never delete user-written custom sections.
- Always preserve the template header layout.
- Adapt section names to the actual project (e.g., "Taskfile commands" vs "Makefile commands" based on what exists).
- Keep the README concise and scannable.
