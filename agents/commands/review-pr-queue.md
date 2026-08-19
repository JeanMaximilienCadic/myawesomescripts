# /review-pr-queue - Review Every PR Awaiting My Review

## Purpose
Work the `review-requested:@me` backlog end to end across all repositories and organizations: discover the open PRs that request the user's review, label and assign each one, review every diff against the actual repository, and post one review per PR.

Unlike `/review-pr`, which reviews a single PR in the current repository, this command operates on the user's entire cross-repo review queue and does not assume a local checkout of any of the target repositories.

## Core principle

**Every finding is verified against the repository before it is written down.** A review listing plausible-sounding problems is worse than no review, because the author must disprove each one. Claims about library APIs, framework conventions, missing files, and repository patterns are all checkable in under a minute. Check them.

## Instructions

1. **Discover the queue:**
   - Run the search and recover `owner/repo` from `repository_url` (the search API returns no repository field, and every later call needs the pair):
     ```bash
     gh api -X GET search/issues \
       -f q='is:open is:pr review-requested:@me archived:false' \
       -f sort=updated -f order=desc \
       --jq '.items[] | "\(.repository_url|split("/")|.[-2:]|join("/"))#\(.number)\t\(.user.login)\t\(.title)"'
     ```
   - Record the author login per PR — step 2 needs it.
   - If the queue is empty, report that and stop.

2. **Label and assign each PR:**
   - **Do not use `gh pr edit`.** It fails with `GraphQL: Projects (classic) is being deprecated ... (repository.pullRequest.projectCards)` regardless of the flags passed. Retrying it cannot succeed. Use the REST issues endpoints instead — pull requests are issues:
     ```bash
     gh api -X POST repos/OWNER/REPO/issues/N/assignees -f 'assignees[]=LOGIN'
     gh api -X POST repos/OWNER/REPO/issues/N/labels -f 'labels[]=bug' -f 'labels[]=chore'
     ```
   - **Assignee:** the PR author, who owns the next action while the PR is in review.
   - **Bot authors cannot be assignees.** `dependabot[bot]`, `snyk-bot`, and similar silently no-op. Assign those PRs to the user (the reviewer) instead.
   - **Labels must already exist in that repository.** Run `gh label list -R OWNER/REPO` first and pick only from what is returned. Repositories in different organizations do not share a taxonomy — never assume a label exists because a sibling repo has it.
   - Apply workflow labels (`waiting-on-you`, `waiting-on-author`) only where the repository actually defines them.

3. **Fetch the diffs and budget the work:**
   - Save each diff to a file rather than holding it inline:
     ```bash
     gh api repos/OWNER/REPO/pulls/N -H "Accept: application/vnd.github.v3.diff" > pr_N.diff
     gh api repos/OWNER/REPO/pulls/N \
       --jq '"+\(.additions)/-\(.deletions) files=\(.changed_files) base=\(.base.ref) mergeable=\(.mergeable_state)"'
     ```
   - **Skip lockfiles** — `uv.lock`, `poetry.lock`, `deno.lock`, `package-lock.json`, `Cargo.lock`. They inflate line counts roughly tenfold and contain nothing reviewable. A PR reported as 7,700 lines is often 1,500 lines of actual code.
   - List the changed files per PR (`grep '^+++ ' pr_N.diff`) and decide where to spend attention before reading.

4. **Verify every suspicion before writing it down:**
   Each of the following is a check that should change a finding rather than confirm it. Run the check; do not assert from memory.

   | Suspicion | How to verify | Never |
   |---|---|---|
   | A dependency bump removes an API the code uses | `pip download PKG==VERSION --no-deps`, extract the wheel, grep for the symbol | State from memory which version removed what |
   | An env-var, import, or config convention is wrong | Read `package.json`, `tsconfig.json`, or the existing sibling file | Infer the framework from the directory layout |
   | A schema change has no migration | List the migrations directory on the default branch — it may already be merged | Report a missing file without looking for it |
   | An RLS policy or auth check is too permissive | Read an existing policy in the same repository to establish local convention | Call it wrong without knowing local practice |
   | A value is unbounded or uncapped | Grep the diff for the clamp and check the tests | Report a cost bug that is already handled |

   When verification is genuinely impossible — a vendor API with no access, a claim about a service's live state — **say so in the review and ask for a second confirmation**, rather than asserting or staying silent.

5. **Review, ordered by whether something breaks:**
   - **Blocking bugs** — the feature does not work. State the failing path concretely: which value stays empty, which control stays disabled, which query fails on a fresh database.
   - **Security and multi-tenancy** — `USING (true)` row-level security, `SECURITY DEFINER` functions without a grant restriction, user input reaching a `LIKE`/`ILIKE` pattern, secrets or ARNs in task definitions.
   - **Data correctness** — deletes keyed on a non-unique column, read-then-write races, filters silently dropped when a variable is undefined.
   - **Contract and API shape** — one field name carrying two meanings, mutating GET endpoints, inconsistent behavior between sibling modes.
   - **Everything else** — dead code, orphaned i18n keys, duplicated markup, arbitrary values where design tokens exist.

   Say what is good, specifically, where it is good — not as padding, but so the author knows which judgment calls to keep making.

6. **Confirm with the user before posting:**
   - Reviews are outward-facing and are published under the user's name. Present the findings in the conversation first and wait for approval.
   - Do not post as a batch action without that approval.

7. **Post one review per PR:**
   ```bash
   gh api -X POST repos/OWNER/REPO/pulls/N/reviews \
     -f event=COMMENT -f body="$(cat review_N.md)"
   ```
   - **Always `event=COMMENT`.** `APPROVE` and `REQUEST_CHANGES` are gating decisions belonging to the user. Use them only on an explicit instruction naming that specific PR.
   - Write each review body to its own file and `cat` it in. Inline heredocs mangle the backticks and `$` characters in embedded code snippets.

8. **Report back:**
   - Give the user a table of PR → review URL → one-line summary of what was found.
   - **State coverage honestly.** If a 48-file PR was reviewed by concentrating on its migrations and leaving components unread, say so in the review itself. A scoped review the author can trust beats a comprehensive-sounding one they cannot.

## Common mistakes

| Mistake | Consequence |
|---|---|
| Retrying `gh pr edit` after the projectCards error | Wastes turns on a call that cannot succeed |
| Assigning a `[bot]` author | Silent no-op; the PR looks assigned but is not |
| Applying a label the repository does not define | Fails, or creates a stray label outside the taxonomy |
| Reporting an unverified API-removal claim | The author spends an hour disproving it |
| Reviewing the lockfile diff | Large context spend, zero findings |
| Approving PRs as a batch action | Takes a decision that belonged to the user |
| Claiming full coverage of a large PR that was skimmed | The skipped file is where the bug was |

## Notes
- Requires the GitHub CLI (`gh`) authenticated as the user whose queue is being worked.
- Works across organizations; the queue is defined by `review-requested:@me`, not by the current directory.
- No local checkout of the target repositories is needed — everything goes through `gh api`.
