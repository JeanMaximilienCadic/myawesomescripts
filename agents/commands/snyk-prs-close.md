# Close Snyk PRs

Find and close all open Snyk-related pull requests in the current repository.

## Instructions

1. Use `gh` CLI to list all open PRs that are Snyk-related. Snyk PRs can be identified by:
   - PRs authored by `snyk-bot` or `snyk-io[bot]` or any author containing "snyk"
   - PRs with branches starting with `snyk-`
   - PRs with titles containing "snyk" (case-insensitive)

2. Run the following `gh` command to find candidate PRs:
   ```
   gh pr list --state open --json number,title,author,headRefName --limit 200
   ```

3. Filter the results to only include Snyk-related PRs using the criteria above.

4. If no Snyk PRs are found, report that there are no open Snyk PRs and stop.

5. Before closing, list all the PRs that will be closed (number, title, branch) and show the count.

6. Close each Snyk PR using:
   ```
   gh pr close <number> --delete-branch --comment "Closed automatically: cleaning up Snyk dependency PRs."
   ```
   Use `--delete-branch` to also remove the remote branch. If deleting the branch fails (e.g. branch already deleted or from a fork), that is acceptable — continue closing the remaining PRs.

7. After processing all PRs, print a summary showing:
   - Total PRs closed
   - Any PRs that failed to close (with error details)
