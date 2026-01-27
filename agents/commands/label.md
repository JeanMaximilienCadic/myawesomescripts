# /label - Automatically Label Pull Requests

## Purpose
Automatically apply labels to a pull request by analyzing its title, description, and diff content.

## Instructions

1. **Identify the target PR:**
   - Run `gh pr view --json number,title,body,labels,additions,deletions,files 2>/dev/null` to get the current branch's PR.
   - If no PR exists on the current branch, ask the user for a PR number or URL.
   - Run `gh pr diff <number>` to get the full diff content.

2. **Analyze PR content to determine labels:**
   Examine the PR title, body, and diff to infer appropriate labels from these categories:

   **Type labels** (based on the nature of changes):
   - `feature` - New functionality added
   - `bugfix` - Bug fix or correction
   - `documentation` - Documentation changes only
   - `refactor` - Code restructuring without behavior change
   - `chore` - Maintenance, dependencies, CI/CD changes
   - `test` - Test additions or modifications
   - `performance` - Performance improvements
   - `security` - Security-related changes

   **Scope labels** (based on what area is affected):
   - `aws` - AWS-related scripts or configurations
   - `docker` - Docker/container changes
   - `network` - Network utility changes
   - `system` - System administration scripts
   - `backup` - Backup and file management
   - `development` - Development tools
   - `python` - Python-related changes
   - `ci/cd` - Pipeline or workflow changes

   **Size labels** (based on diff size):
   - `size/xs` - < 10 lines changed
   - `size/s` - 10-50 lines changed
   - `size/m` - 50-200 lines changed
   - `size/l` - 200-500 lines changed
   - `size/xl` - > 500 lines changed

   **Priority labels** (inferred from content):
   - `breaking-change` - If the diff removes or renames public APIs/scripts
   - `needs-review` - Complex changes that warrant careful review

3. **Ensure labels exist in the repo:**
   - Run `gh label list --json name` to get existing labels.
   - For any label that doesn't exist yet, run `gh label create "<name>" --color "<hex>" --description "<desc>"` to create it.
   - Use consistent color scheme:
     - Type labels: blue tones (`#0075ca`, `#1d76db`)
     - Scope labels: green tones (`#0e8a16`, `#2ea44f`)
     - Size labels: yellow tones (`#fbca04`, `#e4e669`)
     - Priority labels: red/orange tones (`#d93f0b`, `#b60205`)

4. **Apply labels:**
   - Show the user the proposed labels with reasoning for each.
   - Ask the user to confirm or modify the label set.
   - Run `gh pr edit <number> --add-label "<label1>,<label2>,..."`.

5. **Report:**
   - Display the final set of labels applied to the PR.

## Requirements
- `gh` CLI must be authenticated.
- A PR must exist or be specified.
