---
description: Fix a large batch of GitHub issues without exhausting the machine — group issues into few PRs, scan the LAN, run each group's builds/tests in a capped ephemeral LXC slot, one PR per group, merge on green
argument-hint: "<owner/repo | project slice URL | issue URLs> [issue numbers] [--label L] [--milestone M] [--limit N] [--max-group 6] [--slot cpu:mem] [--docker] [--dry-run]"
---

# /batch-fix — batched, LAN-dispatched issue resolution

Two root causes made past sweeps exhaust the machine. First, every build and test ran on one host. Second, every issue got
its own worktree, test runs and PR. This command fixes both: **few PRs (one per group)** and **builds/tests on capped LAN slots**.
Load the `lan-dispatch` skill first; it documents `ldw` (the `LDW` path below).

Decide by yourself and don't stop for confirmations. Only real blockers go back to the user:
secrets, admin merges, sudo on a host.

```
LDW=~/.claude/skills/lan-dispatch/scripts/ldw
```

## Arguments — `$ARGUMENTS`
- `owner/repo` (all open issues), a project slice URL (same parsing as `/project-repo-fix`), or issue URLs or numbers.
- `--label`, `--milestone`: filters. `--limit N`: cap on issues taken this run.
- `--max-group N`: most issues in one PR (default **6**). `--slot cpu:mem`: slot size (default by language, see the skill).
- `--docker`: the tests need Docker. `--dry-run`: stop after printing the grouping and the dispatch plan.

## Step 1 — Snapshot (one API read)
Read all candidate issues in **one paginated GraphQL query**. Fields: number, title, body, labels, milestone, assignees,
`subIssues`, `trackedIssues`/`blockedBy`, and linked open PRs. Save the result to the scratchpad as `issues.json` and work from that file
(this keeps GitHub API usage low). Skip issues that already have an open PR or a `blocked` label.

## Step 2 — Group (this is where the savings are)
Clone or fetch the repo (`git fetch origin`; research from `origin/<default>`, never a stale checkout). For each issue, find
the files and modules it touches: paths in the body, symbols grepped in the code, labels such as `area:*`. Then build groups with these rules:
1. **Hard links first.** Sub-issues of one parent, and issues that block one another, go in the same group.
2. **Same files or module**: group them. One PR beats several PRs that conflict with each other.
3. **Same test surface**: if two issues need the same slow suite, group them. The suite then runs once.
4. **Kept separate**: P0, security, money-moving and migration issues each get their own PR. So do issues whose fix is uncertain
   or large, estimated at more than ~400 changed lines.
5. Each group holds at most `--max-group` issues. Order groups by priority, then by size, smallest first.

6. Groups that share an API contract (backend + the UI that consumes it) can run in parallel. Write the contract (JSON shapes)
   once and paste it verbatim into both briefs. Groups that build on another group's code (a UI shell, then its scenes) go in a later wave.

Before dispatching, find the repo's **cross-checks**: tests that pin one side's files from the other side (citation/line-pin
tests, OpenAPI contract snapshots, generated-type checks). Read the CI workflow for them, and put them in every brief.

Write `groups.json`: `[{id, title, issues:[n...], files:[...], suite:"<full test command>", targeted:"<fast test cmd>", rationale}]`.
Print a table: group, issues, files, suite. Report the reduction ("41 issues → 9 PRs, 9 full-suite runs instead of 41").
`--dry-run` stops here after the Step 3 scan.

## Step 3 — Scan the LAN and reserve slots
```
$LDW scan --cpu C --mem M          # read the "free" column; hosts that are unreachable or under pressure show 0
$LDW up --run <repo>-<MMDD> --count <min(len(groups), total_free)> --cpu C --mem M [--docker]
```
If 0 slots are free anywhere, do **not** fall back to local builds. Wait 10 min (Monitor until-loop on `ldw scan --json`),
or run with at most 1 local worker using `nice -n 19` and `-n 2` test parallelism. Say which in the report.

## Step 4 — Worktrees (local, light)
One worktree per **group**:
`git worktree add ~/<repo>-worktrees/<group-id> -b fix/<group-id> origin/<default>`.
No dependency install on the Mac: `node_modules`, `.venv` and `target` live only in the slots.

## Step 5 — Dispatch workers (one per slot; queue the rest)
Start one background Agent per reserved slot, each with one group. When an agent finishes, `$LDW reset <slot>` and start
the next queued group on that slot. Concurrency = number of slots, never more.

Worker brief (fill in the placeholders; keep the rules verbatim):
```
Fix issues #A #B #C of <owner/repo> in worktree <WT> (branch fix/<group-id>). Slot: <SLOT>.
Issues: <title + key body points + files for each>.
RULES
- Builds, tests, linters, type-checks and installs run ONLY via:
    ~/.claude/skills/lan-dispatch/scripts/ldw run <SLOT> <WT> [--timeout S] -- '<cmd>'
  Never run them on this machine. Locally you may: git, gh (your own PR only, at most one call per 2 min), read/edit files,
  sub-second formatters. If a command generates files you need (lockfile, snapshot):
    ldw pull <SLOT> <WT> <path>
- Iterate with the targeted tests: <targeted>. Run the full suite ONCE before pushing: <suite>.
  Also run <cross-checks>: tests on the OTHER side that pin your files (e.g. a frontend test that pins backend
  line numbers, so a backend-only PR still turns the frontend red).
- Merge (never rebase) origin/<default> before the final suite run; siblings of this batch merge while you work.
- One commit per issue (message "fix: <title> (#n)"), no Co-Authored-By, no git stash (WIP commits instead).
  Commit identity: <the git identity required for this repo, if any>.
- If one issue in the group turns out bigger or blocked, leave it out of this PR and say why. Don't stall the group.
- Push, open ONE PR: title "<group title>", body = per-issue summary + "Closes #A, closes #B, ..." + test evidence
  (the ldw exit lines) + "🤖 Generated with [Claude Code](https://claude.com/claude-code)".
- Watch only your PR's checks (gh pr checks, at most once every 2 min). Fix red CI from `gh run view --log-failed`, reproducing
  in the slot. Up to 3 rounds.
- Report: STATUS green|red|partial, PR number, issues closed / dropped (with reason), slot time used.
```

## Step 6 — Merge and close out
- Merge each green PR right away after a `git merge-tree --write-tree origin/<default> origin/fix/<group-id>`
  conflict check. On a conflict, the worker of the second PR merges the default branch
  into its branch and reruns the suite in its slot. Review the diff in depth only for P0, security or money-moving changes.
- Repos with their own rules win (e.g. some repos get a PR only, no merge).
- When every group is done: `$LDW down --run <id>`, then `$LDW list` must show nothing for the run. Remove the merged worktrees.

## Report
Print a table: group → issues → PR → status → slot@host → full-suite time. List the issues that were dropped or blocked, with reasons.
Include the reduction line (issues → PRs, suite runs saved) and the peak slots per host.
