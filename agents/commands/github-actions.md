# /github-actions - Create or Update GitHub Actions CI/CD Workflow

## Purpose
Create or update a GitHub Actions workflow that builds a Docker image, runs tests inside the container, and publishes the Python wheel to PyPI — all driven by Taskfile targets.

## Pipeline

The workflow must follow this job chain:

```
version-check -> test -> publish
```

- **version-check**: Ensures the current `__version__` is not already on PyPI.
- **test**: Builds the Docker image and runs the test suite inside the container.
- **publish**: Builds the wheel via Taskfile and uploads it with `uvx twine`.

## Instructions

### 1. Verify Taskfile prerequisites

Before writing the workflow, check that the required Taskfile targets exist:

- `deploy:version-check` — in `taskfiles/deploy.yml`
- `build:wheel` — in `taskfiles/build.yml`
- `test:docker` or a `test` service in `docker-compose.yml`

If any are missing, invoke the `/taskfile` skill to create or update the Taskfile structure first, then continue.

### 2. Verify Docker prerequisites

Check that the following exist and are properly configured:

- **`Dockerfile`**: Must install the package and `pytest`. Expected lines:
  ```dockerfile
  RUN uv pip install --system .
  RUN uv pip install --system pytest
  ```
- **`docker-compose.yml`**: Must have a `test` service that runs pytest:
  ```yaml
  test:
    build: .
    image: <project-image>
    command: python -m pytest tests/ -v
  ```

If the Dockerfile is missing the pytest install or docker-compose.yml is missing the test service, add them.

### 3. Create or update the workflow

Write `.github/workflows/publish.yml` with the following structure:

```yaml
name: Publish to PyPI

on:
  push:
    branches: [main, master]

jobs:
  version-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Task
        uses: arduino/setup-task@v2
        with:
          version: 3.x

      - name: Check version not already on PyPI
        run: task deploy:version-check

  test:
    runs-on: ubuntu-latest
    needs: version-check
    steps:
      - uses: actions/checkout@v4

      - name: Build and run tests in Docker
        run: docker compose build && docker compose run --rm test

  publish:
    runs-on: ubuntu-latest
    needs: test
    steps:
      - uses: actions/checkout@v4

      - name: Install uv
        uses: astral-sh/setup-uv@v4

      - name: Install Task
        uses: arduino/setup-task@v2
        with:
          version: 3.x

      - name: Build wheel
        run: task build:wheel

      - name: Publish to PyPI
        run: uvx twine upload dist/*.whl
        env:
          TWINE_USERNAME: __token__
          TWINE_PASSWORD: ${{ secrets.PYPI_API_KEY }}
```

### 4. Key decisions baked into this workflow

These were learned through CI failures and should not be reverted:

- **Use `uvx twine`**, not `uv pip install twine && twine`. The GitHub Actions Ubuntu runner uses an externally-managed Python, so `uv pip install --system` fails with PEP 668 errors. `uvx` runs twine in an ephemeral environment.
- **Use `arduino/setup-task@v2`** to install the Task CLI.
- **Use `astral-sh/setup-uv@v4`** to install uv (only needed in the publish job).
- **Docker is pre-installed** on `ubuntu-latest` runners — no setup step needed.
- **Secrets**: The workflow expects `PYPI_API_KEY` in the repository secrets.

### 5. Verify

After writing the workflow:
- Confirm the YAML is valid by checking indentation and structure.
- List the job dependency chain and confirm it is: `version-check -> test -> publish`.
- Remind the user to ensure `PYPI_API_KEY` is configured in the repository secrets.

## Rules
- Always gate `publish` on `test` passing.
- Always gate `test` on `version-check` passing.
- Never use `uv pip install` in the publish job — always use `uvx` to run tools.
- Never echo or log secrets. Taskfile tasks that handle tokens must use `silent: true`.
- If Taskfile targets are missing, invoke `/taskfile` to create them before writing the workflow.
