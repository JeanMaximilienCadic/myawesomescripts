# /link-issue - Link GitHub Issues with Pull Requests

## Purpose
Automatically find and link relevant GitHub issues to a pull request by analyzing content similarity between PR diffs and open issues assigned to the current user.

## Instructions

1. **Identify the current user and PR:**
   - Run `gh api user --jq '.login'` to get the current GitHub username.
   - Run `gh pr view --json number,title,body,url` to get the current branch's PR.
   - If no PR exists, inform the user and stop.
   - Run `gh pr diff` to get the full diff content for analysis.

2. **Fetch candidate issues:**
   - Run `gh issue list --assignee @me --state open --json number,title,body,labels --limit 100` to get all open issues assigned to the current user.
   - Also run `gh issue list --author @me --state open --json number,title,body,labels --limit 100` to include issues authored by the user.
   - Deduplicate by issue number.

3. **Check for already-linked issues:**
   - For each candidate issue, run `gh issue view <number> --json body,comments` and check if the PR number or URL is already referenced.
   - Also check if the PR body already contains `Closes #<number>`, `Fixes #<number>`, or `Resolves #<number>`.
   - Skip any issue that is already linked.

4. **Match issues to the PR:**
   - Analyze the PR title, body, and diff content.
   - For each unlinked issue, compare:
     - **Title similarity**: Does the issue title relate to the changes in the PR?
     - **Body/description overlap**: Do keywords, components, or file references match?
     - **Label alignment**: Do issue labels match the type/scope of changes?
   - Score each issue on relevance and rank them.
   - Present the top matches (if any) to the user with a brief explanation of why each matches.

5. **Link matched issues:**
   - Ask the user to confirm which issues to link.
   - For each confirmed issue, update the PR body to append `Closes #<number>` (or `Relates to #<number>` if the user prefers a softer link).
   - Run `gh pr edit <pr-number> --body "<updated-body>"` to update the PR.

6. **If no matching issue is found:**
   - Inform the user that no matching open issue was found.
   - Propose creating a new issue based on the PR content:
     - Draft an issue title and body derived from the PR.
     - Ask the user to confirm or edit.
     - Run `gh issue create --title "<title>" --body "<body>" --assignee @me` to create the issue.
   - After creation, link the new issue to the PR by updating the PR body with `Closes #<new-issue-number>`.

7. **Report:**
   - Display the final linked issues and PR URL.

## Requirements
- `gh` CLI must be authenticated.
- A PR must exist on the current branch.
- The user must have open issues assigned or authored for matching to work.
