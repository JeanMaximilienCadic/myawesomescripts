# /commit - Commit Current Changes

## Purpose
Stage and commit current changes using Conventional Commits format, enforcing a subject line under 72 characters.

## Instructions

1. **Analyze current changes:**
   - Run `git status` to see untracked and modified files.
   - Run `git diff` to see unstaged changes.
   - Run `git diff --cached` to see already staged changes.
   - Run `git log --oneline -10` to review recent commit style for context.

2. **Stage files:**
   - If no files are staged, show the user the list of changed/untracked files.
   - Ask the user which files to stage, or offer to stage all relevant files.
   - Never stage files that likely contain secrets (`.env`, `credentials.json`, `*.pem`, `*.key`). Warn the user if such files are detected.
   - Run `git add <files>` for the selected files.

3. **Generate commit message:**
   - Analyze the staged diff to determine the nature of the changes.
   - Generate a commit message in **Conventional Commits** format:
     ```
     <type>(<optional-scope>): <description>
     ```
   - Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `ci`, `perf`, `build`
   - The **entire first line must be under 72 characters**.
   - The scope is optional and should reflect the area of change (e.g., `aws`, `docker`, `network`).
   - The description must be lowercase, imperative mood, no period at the end.
   - Examples:
     - `feat(aws): add ec2 instance listing support`
     - `fix(docker): resolve image cleanup race condition`
     - `docs: update readme with new script entries`
     - `chore: remove unused backup scripts`

4. **Confirm and commit:**
   - Present the generated commit message to the user.
   - Ask the user to confirm or edit.
   - Run the commit using a HEREDOC:
     ```bash
     git commit -m "$(cat <<'EOF'
     <commit message>

     Co-Authored-By: Claude <noreply@anthropic.com>
     EOF
     )"
     ```

5. **Post-commit:**
   - Run `git status` to verify the commit succeeded.
   - Show the commit hash and message.

## Rules
- NEVER amend a previous commit unless the user explicitly asks.
- NEVER use `--no-verify` unless the user explicitly asks.
- The subject line MUST be under 72 characters. If the generated message is too long, shorten it.
- If there are no changes to commit, inform the user and do nothing.
