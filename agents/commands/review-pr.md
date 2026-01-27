# /review-pr - Review Pull Request for Issues

## Purpose
Analyze an open pull request to identify non-minor issues by reviewing the code diff, verifying Docker image builds, running tests, and flagging problems that could impact production reliability, security, or correctness.

## Instructions

1. **Identify the pull request:**
   - If a PR number is provided as an argument, use it directly.
   - If no PR number is provided, check if the current branch has an open PR using `gh pr view --json number,title,body,baseRefName,headRefName`.
   - If no PR is found, list open PRs with `gh pr list` and ask the user which one to review.
   - Read the PR details: title, description, base branch, head branch, and full diff.

2. **Fetch and analyze the diff:**
   - Run `gh pr diff <number>` to get the full diff.
   - Run `gh pr view <number> --json files` to get the list of changed files.
   - For each changed file, understand what was modified and why.
   - Read the full content of significantly changed files (not just the diff) to understand context.

3. **Code review - identify issues:**
   - Focus on **non-minor issues only**. Ignore cosmetic problems (formatting, naming style, comment typos).
   - Check for these categories:
     - **Security:** SQL injection, XSS, command injection, hardcoded secrets, insecure defaults, missing input validation at boundaries.
     - **Correctness:** Logic errors, off-by-one errors, race conditions, null/undefined handling, incorrect error handling, missing edge cases.
     - **Reliability:** Unhandled exceptions, resource leaks (files, connections, locks), missing timeouts, infinite loops, missing retries for network calls.
     - **Breaking changes:** API contract changes, removed public methods/fields, changed return types, schema migrations without backward compatibility.
     - **Performance:** O(n^2) or worse in hot paths, missing pagination, unbounded queries, memory leaks, loading large data into memory.
     - **Dependencies:** New dependencies with known vulnerabilities, incompatible version constraints, unnecessary dependencies.
   - For each issue found, note:
     - File and line number.
     - Severity: `critical`, `major`, or `moderate` (skip minor/cosmetic).
     - Clear description of the problem.
     - Suggested fix if applicable.

4. **Verify Docker image builds:**
   - Find all Dockerfiles in the repository (root `Dockerfile`, `docker/Dockerfile`, `*.dockerfile`, Docker Compose files).
   - For each Dockerfile affected by the PR changes (or that depends on changed files):
     - Attempt to build it: `docker build -f <dockerfile> .`
     - If the build fails, capture the error output and report it as a critical issue.
     - If the build succeeds, report success.
   - If no Dockerfiles exist or none are affected, skip this step and note it in the report.
   - If Docker is not available, skip and note that Docker verification was not possible.

5. **Run tests:**
   - Detect the test framework by checking for:
     - `pytest.ini`, `pyproject.toml` with `[tool.pytest]`, `setup.cfg` with `[tool:pytest]` -> run `pytest` or `uv run pytest`.
     - `package.json` with test script -> run `npm test`.
     - `Taskfile.yml` with a test task -> run `task test`.
     - `Makefile` with a test target -> run `make test`.
     - `go.mod` -> run `go test ./...`.
   - Run the detected test suite and capture results.
   - If tests fail, report each failure as an issue with:
     - Test name and file.
     - Failure message.
     - Whether the failure is likely caused by the PR changes or is a pre-existing issue.
   - If no test framework is detected, note this as a moderate issue (missing test coverage).
   - If tests pass, report success and note the number of tests run.

6. **Check for missing tests:**
   - For new functions/methods/endpoints added in the PR, check if corresponding tests exist.
   - If new logic is added without tests, flag it as a moderate issue.

7. **Generate the review report:**
   - Present the findings in a structured format:
     ```
     ## PR Review: #<number> - <title>

     ### Summary
     <1-2 sentence overview of what the PR does and overall assessment>

     ### Docker Build
     - [ ] <Dockerfile>: <PASS/FAIL> <details if failed>

     ### Tests
     - [ ] <Test suite>: <PASS/FAIL> (<N> tests, <M> failures)

     ### Issues Found

     #### Critical
     - **<file>:<line>** - <description>
       <details and suggested fix>

     #### Major
     - **<file>:<line>** - <description>

     #### Moderate
     - **<file>:<line>** - <description>

     ### No Issues
     <If no issues found, state that the PR looks good>
     ```

8. **Ask about posting the review:**
   - Ask the user if they want to:
     - Post the review as a PR comment using `gh pr comment <number> --body "..."`.
     - Post as a formal review using `gh pr review <number> --comment --body "..."`.
     - Request changes using `gh pr review <number> --request-changes --body "..."`.
     - Just display the results without posting.

## Rules
- Never approve a PR automatically - only the user decides to approve.
- Focus on non-minor issues. Do not flag style preferences, naming conventions, or formatting unless they cause actual problems.
- Always verify Docker builds and run tests before concluding the review - do not skip these steps unless the tools are unavailable.
- If the test suite takes more than 5 minutes, let the user know and ask whether to wait or skip.
- Be specific in issue descriptions - include file paths, line numbers, and concrete examples of what could go wrong.
- Distinguish between issues introduced by the PR and pre-existing issues. Focus the review on PR-introduced problems.
- If the PR is too large (more than 50 files changed), warn the user that a thorough review may be difficult and suggest reviewing in smaller chunks.
- Do not block on cosmetic or subjective issues - only flag things that could cause bugs, outages, security problems, or breaking changes.
