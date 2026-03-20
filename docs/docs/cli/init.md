# skippr-dbt init

Initialise a new project in the current directory.

## Usage

```bash
skippr-dbt init <project-name>
```

## What it does

- Creates `skippr-dbt.yaml` with the project name.
- Creates `.env.example` listing the required environment variables.

## Arguments

| Argument | Required | Description |
|---|---|---|
| `project-name` | Yes | Project identifier. Used as the pipeline name and default dbt schema prefix. |

## Example

```bash
mkdir my-workspace && cd my-workspace
skippr-dbt init mssql-migration
```

Output:

```
Initialised project 'mssql-migration' in /Users/me/my-workspace

Next steps:
  skippr-dbt connect warehouse snowflake
  skippr-dbt connect source mssql
  skippr-dbt doctor
  skippr-dbt run
```

## Notes

- Running `init` in a directory that already contains `skippr-dbt.yaml` will fail. Delete the existing file first to re-initialise.
- The project name should be a valid identifier (letters, numbers, underscores). It is used to name Snowflake schemas (e.g. `mssql_migration_silver`).
