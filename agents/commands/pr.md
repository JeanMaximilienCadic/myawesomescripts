# /pr - Create or Update a Pull Request

## Purpose
Create a new pull request or update an existing one using conventional PR titles and auto-detected base branch.

## Instructions

1. **Detect current state:**
   - Run `git branch --show-current` to get the current branch name.
   - Run `git remote show origin` or parse `.git/config` to auto-detect the default base branch (usually `main` or `master`).
   - Run `git log <base>..HEAD --oneline` to list all commits that will be part of the PR.
   - Run `git diff <base>...HEAD --stat` to summarize changed files.
   - Run `gh pr view --json number,title,body,state 2>/dev/null` to check if a PR already exists for this branch.

2. **If a PR already exists:**
   - Show the user the current PR title and body.
   - Ask the user what they want to update (title, body, reviewers, labels, draft status).
   - Run `gh pr edit <number>` with the appropriate flags.

3. **If no PR exists, create one:**
   - Analyze all commits and the diff to draft a PR title and body.
   - The PR title **must** follow Conventional Commits format:
     - `feat: ...` for new features
     - `fix: ...` for bug fixes
     - `docs: ...` for documentation changes
     - `chore: ...` for maintenance tasks
     - `refactor: ...` for code refactoring
     - `test: ...` for test additions/changes
     - `ci: ...` for CI/CD changes
     - `style: ...` for formatting changes
   - The PR body must follow this template:
     ```
     ## Summary
     <1-3 bullet points describing the changes>

     ## Changes
     <list of notable changes with file references>

     ## Test plan
     - [ ] <testing checklist items>
     ```
   - Ask the user to confirm or edit the title and body before creating.
   - Run `gh pr create --title "<title>" --body "<body>" --base <base-branch>`.
   - If the user wants a draft PR, add `--draft`.

4. **Post-creation:**
   - Display the PR URL.
   - Ask if the user wants to add reviewers or labels.

## Requirements
- `gh` CLI must be authenticated.
- The current branch must be pushed to the remote.
- If not pushed, offer to push with `git push -u origin <branch>`.
