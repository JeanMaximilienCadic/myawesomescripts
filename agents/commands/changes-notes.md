# /changes-notes - Create or Update CHANGELOG

## Purpose
Maintain a single `CHANGELOG.md` at the repository root following the [Keep a Changelog](https://keepachangelog.com/) format, grouped by version and date.

## Format
The CHANGELOG must follow this structure:

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- <new features>

### Changed
- <changes to existing functionality>

### Deprecated
- <soon-to-be removed features>

### Removed
- <removed features>

### Fixed
- <bug fixes>

### Security
- <vulnerability fixes>

## [X.Y.Z] - YYYY-MM-DD

### Added
- ...
```

## Instructions

1. **Check existing state:**
   - Read `CHANGELOG.md` if it exists.
   - Run `git tag --sort=-v:refname` to list existing version tags.
   - Run `git log --oneline` to get the full commit history.
   - If tags exist, run `git log <last-tag>..HEAD --oneline` to get commits since last release.
   - If no tags exist, use all commits.

2. **Classify commits:**
   - Parse each commit message looking for Conventional Commits prefixes:
     - `feat:` -> **Added**
     - `fix:` -> **Fixed**
     - `docs:` -> **Changed** (documentation)
     - `refactor:` -> **Changed**
     - `perf:` -> **Changed** (performance)
     - `test:` -> **Changed** (tests)
     - `chore:` -> **Changed** (maintenance)
     - `BREAKING CHANGE` or `!:` -> **Changed** (with breaking change note)
     - `deprecate:` -> **Deprecated**
     - `remove:` -> **Removed**
     - `security:` -> **Security**
   - For non-conventional commits, analyze the message to best-fit a category.

3. **Update or create CHANGELOG.md:**
   - If the file doesn't exist, create it with the full template and populate `[Unreleased]` with classified commits.
   - If the file exists:
     - Parse existing entries to avoid duplicates.
     - Add new commits under `[Unreleased]`.
     - If the user is creating a release, ask for the version number and move `[Unreleased]` items to a new versioned section with today's date.

4. **Present changes:**
   - Show the user the new/updated entries.
   - Ask the user to confirm before writing.

5. **Write `CHANGELOG.md`.**

## Rules
- Never remove existing changelog entries.
- Always keep an `[Unreleased]` section at the top.
- Entries should be human-readable, not raw commit hashes.
- Each entry should start with a verb in past tense (e.g., "Added support for...", "Fixed crash when...").
- Group related changes together.
- Omit empty categories (don't include `### Deprecated` if there are no deprecated items).
