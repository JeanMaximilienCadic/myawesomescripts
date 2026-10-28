---
description: Fix a large batch of GitHub issues without exhausting the machine — group issues into few PRs (reviving stale ones), inventory the LAN, print a wave-by-wave dispatch plan (issues, PR, host, ETA), run each group's builds/tests in a capped slot, one PR per group, merge on green
argument-hint: "<owner/repo | project slice URL | issue URLs> [issue numbers] [--label L] [--milestone M] [--limit N] [--max-group 6] [--slot cpu:mem] [--docker] [--dry-run]"
---

# /batch-fix — batched, LAN-dispatched issue resolution

Two root causes made past sweeps exhaust the machine. First, every build and test ran on one host. Second, every issue got
its own worktree, test runs and PR. This command fixes both: **few PRs (one per group)** and **builds/tests on capped LAN slots**.
Load the `lan-dispatch` skill first if it's installed; it documents `ldw` (the `LDW` path below). Step 3 covers running without it.

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

Batch as many issues per PR as is logical, and take as many issues as can be done. Only leave out issues that can't be
done: epics and trackers, open decisions and ADR proposals, issues blocked by an open upstream issue, and issues that
need production access. Check each `blockedBy` link; a closed blocker means the issue can be taken.

Look for work that already exists before planning new work:
- **Your own stale PRs** (drafts, conflicting, or red, with no activity for days) that close candidate issues. Plan a
  *revive* group for each: merge the default branch in, fix the checks, mark it ready. Count the issues it closes.
- **Pushed branches with no PR** and **unpushed local worktree commits** for candidate issues. Resume from them.
- **Epic branches** (`epic/*`) where sub-issues are being built. Plan groups that target that branch, not the default branch.

Write `groups.json`: `[{id, title, issues:[n...], files:[...], suite:"<full test command>", targeted:"<fast test cmd>",
kind:"api|web|docs", base:"<default or epic/*>", revive:<PR# or null>, wave:n, gate:"<what must merge first>", eta_h:n, rationale}]`.
Print a table: group, issues, files, suite. Report the reduction ("41 issues → 9 PRs, 9 full-suite runs instead of 41").
Also list the issues that were left out, each with its reason.

## Step 3 — Dispatch plan (LAN inventory and waves)
Run this before any slot is reserved. `--dry-run` stops after printing it.

**Inventory.** Probe every host in `~/.ssh/config` (and the jump-host paths in CLAUDE.md), read-only, with
`ssh -o BatchMode=yes -o ConnectTimeout=6`: cpu, load, available memory, free disk, docker / compose / lxc / node /
python versions, npm and GitHub egress. If a host doesn't answer, check its config address and say why it failed
(no route, no DNS, host key not verified). Never accept a host key yourself; ask the user to check it.
Print a capacity table: host | cpu | load | mem avail | slots | note.
- Never use hosts that run production or staging infrastructure (ArgoCD, edge proxies, CI runners, k3s) as slots.
- A host without the runtime the suite needs, or below the required language version, gets no slot.
- API slots need docker compose (DB fixtures). A host with only node gets web-only slots.
- Slot sizes: API 3 cpu / 6 GB, web 2–3 cpu / 4–6 GB. Leave headroom: no new slot when load > 0.75 × cpu, or
  available memory < 2 × slot memory.

**Leaked slots.** List `ldw-*` instances on every host (`lxc list --all-projects`, `docker ps -a --filter label=ldw.run`),
with `user.ldw.run`, `user.ldw.expires` and the `/work/*` directory inside each. For an expired slot, check whether its
work is on a branch or a merged PR. Work that exists only in the slot is a **salvage** step, done before anything else:
copy it to a local worktree and push it as a WIP commit. Deleting a slot needs the user's OK. Leave slots from other runs
or repos alone.

**Without `ldw`.** If `ldw` isn't installed on any reachable host, start slots by hand in the same shape:
an LXC slot from the `ldw-cache` image (`limits.cpu`, `limits.memory`, `security.nesting=true` for API), a capped
`docker run --cpus --memory` container, or a `nice -n 19` directory with `--maxWorkers=2` on a host with only node.
Label every slot with `ldw.run=<repo>-<MMDD>`, owner and an expiry 24h out. Sync with tar over ssh. Git, gh and edits
stay on the controller.

**Waves.** Assign each group a slot that fits its `kind`. Wave 1 is everything without a gate. A gate is a merge the
group needs (a dependency, the open PR that touches the same files, a revived PR). Groups that only share a contract
can run in the same wave if the contract is in both briefs. Two revive groups on the same files run one after the other
on the same slot.

Print one table per wave, with the ETAs counted from **T0** (the start of wave 1) and including ~30 min of PR CI:

| Batched issues | PR | Host / slot | ETA (done) |
|---|---|---|---|
| #515, #545 | new: <title> | x399 API-1 | ~T0+1.5h |
| #631, #632, … | revive **#649** | msi API-5 | ~T0+1.5h |

Then print: peak workers per host, total (issues → PRs, of which revived), the overall ETA, the least certain rows, and
**Needs your OK**: the slots to start, salvage steps, drafts to mark ready, and slots to delete. Save the plan to the
scratchpad as `dispatch-plan.md`. Wait for the user's OK on that list before Step 4; nothing else needs one.

## Step 4 — Reserve slots
With `ldw` (otherwise start the Step 3 slots by hand, in the same shape):
```
$LDW scan --cpu C --mem M          # read the "free" column; hosts that are unreachable or under pressure show 0
$LDW up --run <repo>-<MMDD> --count <min(len(groups), total_free)> --cpu C --mem M [--docker]
```
If 0 slots are free anywhere, do **not** fall back to local builds. Wait 10 min (Monitor until-loop on `ldw scan --json`),
or run with at most 1 local worker using `nice -n 19` and `-n 2` test parallelism. Say which in the report.

## Step 5 — Worktrees (local, light)
One worktree per **group**:
`git worktree add ~/<repo>-worktrees/<group-id> -b fix/<group-id> origin/<default>`.
No dependency install on the controller: `node_modules`, `.venv` and `target` live only in the slots.

## Step 6 — Dispatch workers (one per slot; queue the rest)
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

## Step 7 — Merge and close out
- Merge each green PR right away after a `git merge-tree --write-tree origin/<default> origin/fix/<group-id>`
  conflict check. On a conflict, the worker of the second PR merges the default branch
  into its branch and reruns the suite in its slot. Review the diff in depth only for P0, security or money-moving changes.
- Repos with their own rules win (e.g. some repos get a PR only, no merge, or an automerge workflow that merges green PRs).
- PRs into `epic/*` branches are not covered by automerge: merge them after the same conflict check.
- Security and money-moving PRs get the `do-not-merge` label until you've reviewed them in depth.
- When every group is done: `$LDW down --run <id>`, then `$LDW list` must show nothing for the run. Remove the merged worktrees.

## Report
Print the Step 3 wave tables again, with the PR numbers filled in and a status column: green | red | partial | queued,
plus the full-suite time. List the issues that were dropped or blocked, with reasons.
Include the reduction line (issues → PRs, suite runs saved) and the peak slots per host.
