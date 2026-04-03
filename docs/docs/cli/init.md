# skippr init

Initialise a new project in the current directory.

## Usage

```bash
skippr init <project-name>
skippr init <project-name> --reset
```

## What it does

- Creates `skippr.yaml` with the project name.
- Creates `.env.example` listing the required environment variables.
- Creates a local `.skippr/` runtime directory in the current working directory.

## Reset an existing project

Use `--reset` when you want to fully re-initialise a project and clear any previous run state:

```bash
skippr init mssql-migration --reset
```

`--reset` will:

- Delete the local `.skippr/` directory (offsets, metadata, buffers).
- Delete remote project metadata and state from the authenticated project's S3 scope.
- Prompt for confirmation by requiring you to type `yes`.
- Recreate the local `.skippr/` environment scaffold.

Your `skippr.yaml` config file is preserved so you don't lose warehouse and source connection settings.

## Arguments

| Argument | Required | Description |
|---|---|---|
| `project-name` | Yes | Project identifier. Used as the pipeline name and default dbt schema prefix. |

## Example

```bash
mkdir my-workspace && cd my-workspace
skippr init mssql-migration
```

Output:

```
Initialised project 'mssql-migration' in /Users/me/my-workspace

Next steps:
  skippr connect warehouse snowflake
  skippr connect source mssql
  skippr doctor
  skippr run
```

## Notes

- Running `init` in a directory that already contains `skippr.yaml` is idempotent. It will print that the project is already initialised and leave the existing files alone.
- Use `skippr init <project-name> --reset` if you want to wipe local and remote state and start again from a clean project.
- The project name should be a valid identifier (letters, numbers, underscores). It is used to name Snowflake schemas (e.g. `mssql_migration_silver`).
