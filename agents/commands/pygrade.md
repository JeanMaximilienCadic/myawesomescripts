# /pygrade - Migrate Python Packaging to pyproject.toml with uv

## Purpose
Migrate any existing `setup.py`, `setup.cfg`, or custom Python packaging configuration into a modern `pyproject.toml` using [uv](https://docs.astral.sh/uv/) as the build and dependency management tool. Also update any Dockerfiles to install and build with `uv`.

## Instructions

1. **Discover existing packaging configuration:**
   - Check for `setup.py` and read it completely to extract: name, version, description, author, license, dependencies, python_requires, packages, entry_points, classifiers, and any custom logic.
   - Check for `setup.cfg` and read it completely.
   - Check for an existing `pyproject.toml` - if found, read it and determine what needs updating (it may already have partial configuration).
   - Check for `requirements.txt`, `requirements-dev.txt`, `requirements/*.txt` and read all dependency files.
   - Check for `MANIFEST.in`, `packages.json`, or any other packaging-related files.
   - Check for `Dockerfile`, `docker/Dockerfile`, `*.dockerfile`, or any Docker Compose files that reference Python installation steps.

2. **Build the pyproject.toml:**
   - Use the `[build-system]` section with `hatchling` as the build backend (uv-compatible):
     ```toml
     [build-system]
     requires = ["hatchling"]
     build-backend = "hatchling.build"
     ```
   - Create the `[project]` section mapping all metadata from setup.py/setup.cfg:
     - `name`: Package name.
     - `version`: Package version (or use dynamic versioning if the project uses `__version__`).
     - `description`: From setup.py description or short_description.
     - `readme`: Point to README.md if it exists.
     - `license`: Use SPDX identifier or `license-files` if custom.
     - `requires-python`: From python_requires.
     - `authors`: From author/author_email.
     - `classifiers`: Migrate classifiers list.
     - `dependencies`: Merge install_requires and requirements.txt.
     - `[project.optional-dependencies]`: Map extras_require if present.
   - If the project has entry_points or console_scripts, add `[project.scripts]`.
   - If the project has a packages list or custom package discovery, add `[tool.hatch.build.targets.wheel]` with the appropriate `packages` configuration.
   - Add `[tool.uv]` section if there are dev dependencies:
     ```toml
     [dependency-groups]
     dev = ["pytest", ...]
     ```

3. **Migrate requirements files into pyproject.toml:**
   - Collect all dependency files: `requirements.txt`, `requirements-dev.txt`, `requirements-test.txt`, `requirements-*.txt`, `requirements/*.txt`.
   - Classify each dependency into a logical group based on its purpose:
     - **Core dependencies** (`dependencies`): Packages required at runtime. These come from `install_requires` in setup.py or the main `requirements.txt` if it only contains runtime deps.
     - **Dev dependencies** (`[dependency-groups]` -> `dev`): Linters, formatters, type checkers, pre-commit tools (e.g., `ruff`, `black`, `flake8`, `mypy`, `pylint`, `isort`, `pre-commit`).
     - **Test dependencies** (`[dependency-groups]` -> `test`): Test frameworks and utilities (e.g., `pytest`, `pytest-cov`, `pytest-mock`, `coverage`, `tox`, `nox`, `hypothesis`, `factory-boy`, `faker`).
     - **Docs dependencies** (`[project.optional-dependencies]` -> `docs`): Documentation tools (e.g., `sphinx`, `mkdocs`, `mkdocstrings`, `pdoc`, `furo`).
     - **CI/Build dependencies** (`[dependency-groups]` -> `build`): Build tools beyond the build backend (e.g., `twine`, `build`, `wheel`, `setuptools-scm`).
   - If a `requirements-dev.txt` or `requirements/dev.txt` exists, use its contents for the `dev` group directly.
   - If a `requirements-test.txt` or `requirements/test.txt` exists, use its contents for the `test` group.
   - If only a single `requirements.txt` exists with mixed runtime and dev dependencies, analyze each package name to split them:
     - Known runtime packages stay in `dependencies`.
     - Known dev/test packages go to the appropriate group.
     - If unsure, keep in `dependencies` and note it for the user to review.
   - Preserve version constraints exactly as specified in the requirements files.
   - Handle `-e .` (editable installs), `-r other-file.txt` (recursive includes), `--index-url`, and other pip options:
     - Editable installs: skip (the project itself).
     - Recursive includes: follow and merge.
     - Index URLs and options: note them for the user but do not put them in pyproject.toml (use `[tool.uv.index]` if needed).
   - Example structure:
     ```toml
     [project]
     dependencies = [
         "requests>=2.28",
         "pydantic>=2.0",
     ]

     [project.optional-dependencies]
     docs = [
         "mkdocs>=1.5",
         "mkdocstrings[python]>=0.24",
     ]

     [dependency-groups]
     dev = [
         "ruff>=0.1",
         "mypy>=1.0",
     ]
     test = [
         "pytest>=7.0",
         "pytest-cov>=4.0",
     ]
     ```

4. **Handle special cases (continued):**
   - If `setup.py` contains dynamic logic (reading files, conditional deps), replicate the result statically in pyproject.toml.
   - If `setup.py` reads version from `__init__.py`, configure dynamic versioning:
     ```toml
     [project]
     dynamic = ["version"]

     [tool.hatch.version]
     path = "package_name/__init__.py"
     ```
   - If there are data files or package_data, configure them under `[tool.hatch.build]`.

5. **Update Dockerfiles:**
   - Find all Dockerfiles in the repository.
   - For each Dockerfile that contains Python installation steps (`pip install`, `python setup.py install`, `pip install -e .`, etc.):
     - Add `uv` installation near the top of the build stage:
       ```dockerfile
       COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /usr/local/bin/
       ```
     - Replace `pip install -r requirements.txt` with `uv sync` or `uv pip install -r requirements.txt`.
     - Replace `python setup.py install` with `uv pip install .`.
     - Replace `pip install -e .` with `uv pip install -e .`.
     - Replace `pip install .` with `uv pip install .`.
     - If the Dockerfile uses a virtual environment, adapt to `uv venv` + `uv sync`.
     - Ensure `COPY pyproject.toml .` is present (add it if only `setup.py` was previously copied).
     - If `requirements.txt` was the only dependency source and is now replaced by pyproject.toml, update the COPY and install commands accordingly.
   - Preserve the overall Dockerfile structure and non-Python steps.

6. **Clean up old files:**
   - Ask the user what to do with replaced files:
     - `setup.py`: Delete, rename to `setup.py.bak`, or keep.
     - `setup.cfg`: Delete, rename, or keep.
     - `requirements.txt`: Ask whether to keep (some tools still use it) or remove. If keeping, suggest adding a comment that pyproject.toml is the source of truth.
   - Do NOT delete files without user confirmation.

7. **Verify:**
   - If `uv` CLI is available, run `uv sync --frozen` or `uv pip install . --dry-run` to validate the pyproject.toml.
   - If `uv` is not available, check that the pyproject.toml is valid TOML syntax.
   - Report any issues found.

8. **Present the plan first:**
   - Before making any changes, show the user:
     - What was found (setup.py contents, requirements, Dockerfiles).
     - The proposed pyproject.toml content.
     - The proposed Dockerfile changes (as a diff summary).
     - Which files will be created/modified.
   - Ask the user to confirm before writing any files.

## Rules
- Always use `hatchling` as the build backend unless the user specifies otherwise - it is the most compatible with uv.
- Never delete files without asking the user first.
- Preserve all existing functionality - the migrated pyproject.toml must install the same packages with the same constraints.
- Keep the pyproject.toml clean and well-organized with sections in standard order: `[build-system]`, `[project]`, `[project.optional-dependencies]`, `[project.scripts]`, `[dependency-groups]`, `[tool.*]`.
- When updating Dockerfiles, use the `COPY --from=ghcr.io/astral-sh/uv:latest` pattern for installing uv - it is the officially recommended approach and avoids adding a separate `RUN` layer.
- If a `uv.lock` file exists, do not modify it - let the user run `uv lock` themselves.
- Map `python_requires` to `requires-python` exactly, preserving the version constraint.
- If `requirements.txt` contains pinned versions (==), keep them as-is in dependencies. If it contains ranges (>=, ~=), preserve those too.
