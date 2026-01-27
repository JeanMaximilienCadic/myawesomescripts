# /taskfile - Create a Modularized Taskfile

## Purpose
Create a modularized `Taskfile.yml` using [go-task](https://taskfile.dev/) that replaces any existing `Makefile` and migrates shell scripts into task targets. The Taskfile should be modular, with included sub-taskfiles per domain.

## Target Structure
```
Taskfile.yml              # Root taskfile with includes
taskfiles/
  aws.yml                 # AWS-related tasks
  docker.yml              # Docker tasks
  backup.yml              # Backup and file management tasks
  build.yml               # Package build tasks (wheel, sdist) - auto-added for Python packages
  development.yml         # Development tool tasks
  network.yml             # Network utility tasks
  system.yml              # System administration tasks
  python.yml              # Python tool tasks
```

## Instructions

1. **Discover existing automation:**
   - Check for `Makefile` and read all targets, dependencies, and recipes.
   - Scan all shell scripts in the repository (files with `#!/bin/bash`, `#!/bin/sh`, or `.sh` extension).
   - Read each script to understand its purpose, arguments, and environment variables.
   - Identify any existing `Taskfile.yml` to avoid conflicts.

2. **Design the modular structure:**
   - Create a root `Taskfile.yml` that includes sub-taskfiles from `taskfiles/` directory.
   - Group tasks by the existing directory structure:
     - `aws/` scripts -> `taskfiles/aws.yml`
     - `docker/` scripts -> `taskfiles/docker.yml`
     - `backup/` scripts -> `taskfiles/backup.yml`
     - `development/` scripts -> `taskfiles/development.yml`
     - `network/` scripts -> `taskfiles/network.yml`
     - `system/` scripts -> `taskfiles/system.yml`
     - `python/` scripts -> `taskfiles/python.yml`

3. **Migrate each script into a task:**
   - For each script, create a corresponding task with:
     - `desc`: Brief description of what the task does.
     - `vars`: Map script arguments and environment variables to task variables.
     - `env`: Required environment variables with defaults where sensible.
     - `cmds`: The script commands, either inlined or calling the original script.
     - `preconditions`: Check for required tools (e.g., `aws`, `docker`, `jq`).
     - `aliases`: Short aliases for frequently used tasks.
   - Preserve the original script behavior exactly.

4. **Migrate Makefile targets:**
   - For each Makefile target, create an equivalent task.
   - Map Makefile variables to Taskfile vars.
   - Map Makefile dependencies to Taskfile `deps`.
   - Convert Makefile recipes to `cmds`.

5. **Detect Python package and add wheel build task:**
   - Check if the repository is a Python package by looking for `pyproject.toml`, `setup.py`, or `setup.cfg` at the root.
   - If it is a Python package, add a `build:wheel` task (or `python:wheel` if grouped under python) that builds a wheel using `uv`:
     ```yaml
     wheel:
       desc: Build wheel distribution with uv
       preconditions:
         - sh: command -v uv
           msg: "uv is required but not installed. Install with: curl -LsSf https://astral.sh/uv/install.sh | sh"
         - sh: test -f pyproject.toml
           msg: "pyproject.toml not found. Run /pygrade to migrate your setup.py first."
       vars:
         DIST_DIR: '{{.ROOT_DIR}}/dist'
       cmds:
         - mkdir -p {{.DIST_DIR}}
         - uv build --wheel --out-dir {{.DIST_DIR}}
     ```
   - Also add a `build:sdist` task for source distribution:
     ```yaml
     sdist:
       desc: Build source distribution with uv
       preconditions:
         - sh: command -v uv
           msg: "uv is required but not installed."
         - sh: test -f pyproject.toml
           msg: "pyproject.toml not found."
       vars:
         DIST_DIR: '{{.ROOT_DIR}}/dist'
       cmds:
         - mkdir -p {{.DIST_DIR}}
         - uv build --sdist --out-dir {{.DIST_DIR}}
     ```
   - Add a `build:all` task that builds both wheel and sdist:
     ```yaml
     all:
       desc: Build wheel and source distribution
       deps:
         - wheel
         - sdist
     ```
   - Add a `build:clean` task to remove the dist folder:
     ```yaml
     clean:
       desc: Remove dist directory
       cmds:
         - rm -rf {{.DIST_DIR}}
     ```
   - These build tasks should go in a `taskfiles/build.yml` sub-taskfile, included as:
     ```yaml
     build:
       taskfile: taskfiles/build.yml
       optional: true
     ```
   - The wheel file will be output to the `dist/` folder at the repository root.

6. **Add common utility tasks:**
   - `task setup` - Install prerequisites and verify environment.
   - `task list` - Built-in (just `task --list`).
   - `task help` - Show detailed help for all tasks.

7. **Root Taskfile.yml format:**
   ```yaml
   version: '3'

   includes:
     aws:
       taskfile: taskfiles/aws.yml
       optional: true
     docker:
       taskfile: taskfiles/docker.yml
       optional: true
     # ... etc

   tasks:
     default:
       desc: Show available tasks
       cmds:
         - task --list

     setup:
       desc: Install prerequisites and verify environment
       cmds:
         - echo "Checking prerequisites..."
         # dependency checks
   ```

8. **Present the plan:**
   - Show the user:
     - Which scripts will be migrated and to which sub-taskfile.
     - Which Makefile targets will be converted.
     - The proposed task names and structure.
   - Ask the user to confirm before creating files.

9. **Create the files:**
   - Create `taskfiles/` directory.
   - Write each sub-taskfile.
   - Write the root `Taskfile.yml`.
   - If a `Makefile` was migrated, ask the user if they want to:
     - Delete the Makefile.
     - Rename it to `Makefile.bak`.
     - Keep it alongside the Taskfile.

10. **Verify:**
   - If `task` CLI is available, run `task --list` to verify the Taskfile is valid.
   - Report any issues.

## Rules
- Always use Taskfile version 3 syntax.
- Keep tasks self-documenting with `desc` fields.
- Use `preconditions` to check for required tools rather than failing silently.
- Use `vars` for configurable values, not hardcoded paths.
- Preserve the original scripts - tasks should call them or replicate their behavior.
- Use `optional: true` on includes so the root Taskfile works even if a sub-taskfile is missing.
