# Skippr

Skippr is a data pipeline CLI that extracts data from sources, loads it into a cloud warehouse, and automatically generates and validates dbt models on top.

## What it does

1. **Extract** -- reads tables, files, streams, or APIs from your source systems.
2. **Load** -- writes raw data into a bronze schema in your warehouse.
3. **Model** -- generates, compiles, and materialises silver and gold dbt models using AI-assisted schema mapping.

All data transfer happens locally on your machine. No data leaves your network.

## Supported connectors

### Sources

| Source | Kind |
|---|---|
| Microsoft SQL Server | `mssql` |
| MySQL | `mysql` |
| PostgreSQL | `postgres` |
| Amazon Redshift | `redshift` |
| MongoDB | `mongodb` |
| Amazon S3 | `s3` |
| SFTP | `sftp` |
| Kafka | `kafka` |
| Amazon SQS | `sqs` |
| Amazon Kinesis | `kinesis` |
| Amazon DynamoDB | `dynamodb` |
| ClickHouse | `clickhouse_source` |
| Delta Lake | `delta_lake` |
| MotherDuck | `motherduck_source` |
| AMQP (RabbitMQ) | `amqp` |
| Amazon SNS | `sns` |
| Amazon EventBridge | `eventbridge` |
| HTTP Server | `http_server` |
| HTTP Client | `http_client` |
| MQTT | `mqtt` |
| WebSocket | `websocket` |
| Socket (TCP/UDP/Unix) | `socket` |
| StatsD | `statsd` |
| Local File | `file` |
| Stdin | `stdin` |

### Destinations

| Destination | Kind |
|---|---|
| Snowflake | `snowflake` |
| Google BigQuery | `bigquery` |
| PostgreSQL | `postgres` |
| AWS Athena (S3 + Glue) | `athena` |
| Databricks | `databricks` |
| Azure Synapse | `synapse` |
| Amazon Redshift | `redshift` |
| ClickHouse | `clickhouse` |
| MotherDuck | `motherduck` |
| Google Cloud Storage | `gcs` |
| Azure Blob Storage | `azure_blob` |
| SFTP | `sftp` |
| AMQP (RabbitMQ) | `amqp` |
| Local File | `file` |
| Stdout | `stdout` |

## Quick start

```bash
skippr init my-project
skippr connect warehouse snowflake
skippr connect source mssql
skippr doctor
skippr run
```

See the [Install](getting-started/install.md) and [Quick Start](getting-started/quickstart.md) guides for a detailed walkthrough.

## Requirements

| Dependency | Why |
|---|---|
| Python 3.10+ | Required by dbt |
| dbt-core + warehouse adapter | Model compilation and materialisation |
