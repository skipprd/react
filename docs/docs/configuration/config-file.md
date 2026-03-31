# Config File

Skippr dbt is configured via `skippr.yaml` in the working directory.

## Format

```yaml
project: mssql_migration

warehouse:
  kind: snowflake
  database: ANALYTICS
  schema: RAW
  warehouse: COMPUTE_WH
  role: ACCOUNTADMIN

source:
  kind: mssql
  connection_string: ${MSSQL_CONNECTION_STRING}

dbt:
  target_schema: custom_name
  silver_suffix: silver
  gold_suffix: gold
```

## Fields

### project (required)

The project identifier. Used as:

- The pipeline name for extract-and-load
- The default dbt schema prefix (e.g. `mssql_migration_silver`, `mssql_migration_gold`)

### warehouse (required)

The destination warehouse connection.

#### Athena (S3 + Glue)

| Field | Description |
|---|---|
| `kind` | `athena` |
| `workgroup` | Athena workgroup name (default: primary) |
| `region` | AWS region |
| `result_s3` | S3 output location for query results (e.g. `s3://bucket/athena-results/`). Omit to use the workgroup default. |
| `catalog` | Glue Data Catalog name (default: `AwsDataCatalog`) |
| `schema` | Default database/schema for discovery and unqualified queries |

Authentication uses the AWS default credential chain (env vars, instance profile, SSO).

```yaml
# Athena warehouse
warehouse:
  kind: athena
  workgroup: primary
  region: us-east-1
  result_s3: s3://my-bucket/athena-results/
  catalog: AwsDataCatalog
  schema: my_database
```

#### Snowflake

| Field | Description |
|---|---|
| `kind` | `snowflake` |
| `database` | Snowflake database |
| `schema` | Bronze/raw schema where extracted data lands |
| `warehouse` | Compute warehouse |
| `role` | Snowflake role |

Authentication is via environment variables, not the config file.

#### BigQuery

| Field | Description |
|---|---|
| `kind` | `bigquery` |
| `project` | GCP project ID |
| `dataset` | BigQuery dataset |
| `location` | Dataset location |

#### Postgres

| Field | Description |
|---|---|
| `kind` | `postgres` |
| `database` | PostgreSQL database name |
| `schema` | Target schema (default: `public`) |

Authentication is via environment variables (`POSTGRES_HOST`, `POSTGRES_USER`, etc.), not the config file.

#### Databricks

| Field | Description |
|---|---|
| `kind` | `databricks` |
| `workspace_url` | Databricks workspace URL (or set `DATABRICKS_WORKSPACE_URL`) |
| `token` | Personal access token (or set `DATABRICKS_TOKEN`) |
| `warehouse_id` | SQL warehouse ID (or set `DATABRICKS_WAREHOUSE_ID`) |
| `catalog` | Unity Catalog name (e.g. `main`) |
| `schema` | Target schema for bronze/raw data |

```yaml
# Databricks warehouse
warehouse:
  kind: databricks
  workspace_url: ${DATABRICKS_WORKSPACE_URL}
  token: ${DATABRICKS_TOKEN}
  warehouse_id: ${DATABRICKS_WAREHOUSE_ID}
  catalog: main
  schema: default
```

#### Azure Synapse

| Field | Description |
|---|---|
| `kind` | `synapse` |
| `connection_string` | Optional ADO.NET connection string (or set individual fields via env vars) |
| `schema` | Target schema (default: `dbo`) |

Authentication is via environment variables: `SYNAPSE_HOST`, `SYNAPSE_USER`, `SYNAPSE_PASSWORD`, `SYNAPSE_DATABASE`.

```yaml
warehouse:
  kind: synapse
  schema: dbo
```

#### Amazon Redshift

| Field | Description |
|---|---|
| `kind` | `redshift` |
| `database` | Redshift database name |
| `schema` | Target schema (bronze/raw) |
| `cluster_identifier` | Provisioned cluster ID (or use `workgroup_name` for Serverless) |
| `workgroup_name` | Serverless workgroup name |
| `db_user` | Database user |
| `region` | AWS region |
| `staging_s3_bucket` | Optional S3 bucket for staging |
| `staging_s3_prefix` | Optional key prefix under the staging bucket |
| `iam_role_arn` | Optional IAM role for COPY |

#### ClickHouse

| Field | Description |
|---|---|
| `kind` | `clickhouse` |
| `url` | HTTP interface URL (e.g. `http://localhost:8123`; or set `CLICKHOUSE_URL`) |
| `database` | Database name |
| `user` | Username |
| `password` | Password |

```yaml
warehouse:
  kind: clickhouse
  url: http://localhost:8123
  database: default
  user: default
```

#### MotherDuck

| Field | Description |
|---|---|
| `kind` | `motherduck` |
| `motherduck_token` | MotherDuck auth token (required) |
| `database` | MotherDuck database name |
| `schema` | Target schema |

```yaml
warehouse:
  kind: motherduck
  motherduck_token: ${MOTHERDUCK_TOKEN}
  database: my_database
  schema: main
```

### schema_sink (optional)

When set, pipeline metadata schemas are also registered with an AWS Glue Data Catalog database (for example to pair with Athena).

#### Glue

| Field | Description |
|---|---|
| `kind` | `glue` |
| `glue_database_name` | Glue database name |

```yaml
# Glue schema sink
schema_sink:
  kind: glue
  glue_database_name: my_database
```

### source (optional)

The data source for extraction. When absent, the pipeline skips extraction and starts at the modeling phase.

#### Databases

##### MSSQL

| Field | Description |
|---|---|
| `kind` | `mssql` |
| `connection_string` | ADO.NET connection string. Use `${ENV_VAR}` to reference environment variables. |

##### MySQL

| Field | Description |
|---|---|
| `kind` | `mysql` |
| `connection_string` | MySQL connection string (e.g. `mysql://user:pass@host:3306/db`). |

##### Postgres

| Field | Description |
|---|---|
| `kind` | `postgres` |
| `host` | Postgres host (default: `localhost`) |
| `port` | Postgres port (default: `5432`) |
| `user` | Username |
| `password` | Password |
| `database` | Database name |
| `connection_string` | Full connection string (overrides individual fields) |
| `tables` | Optional list of tables to read |

##### Redshift

| Field | Description |
|---|---|
| `kind` | `redshift` |
| `cluster_identifier` | Redshift cluster identifier |
| `database` | Database name |
| `db_user` | Database user |
| `tables` | Optional list of tables to read |
| `region` | AWS region |

##### MongoDB

| Field | Description |
|---|---|
| `kind` | `mongodb` |
| `connection_string` | MongoDB connection URI |
| `database` | Database name |
| `collection` | Collection name |
| `filter` | Optional JSON filter document |

##### ClickHouse

| Field | Description |
|---|---|
| `kind` | `clickhouse_source` |
| `url` | HTTP interface URL |
| `database` | Database name |
| `user` | Username |
| `password` | Password |
| `tables` | Optional list of tables |
| `query` | Optional SQL query instead of table list |

##### MotherDuck

| Field | Description |
|---|---|
| `kind` | `motherduck_source` |
| `motherduck_token` | MotherDuck auth token (required) |
| `database` | MotherDuck database name |
| `tables` | Optional list of tables |
| `query` | Optional SQL query |

#### Object Stores & Files

##### S3

| Field | Description |
|---|---|
| `kind` | `s3` |
| `s3_bucket` | S3 bucket name |
| `s3_prefix` | Key prefix |

##### Delta Lake

| Field | Description |
|---|---|
| `kind` | `delta_lake` |
| `table_uri` | Table location URI (e.g. `s3://bucket/path/to/table`) |
| `storage_options` | Optional map of storage credentials/options |
| `version` | Optional table version |
| `filter` | Optional filter expression |

##### SFTP

| Field | Description |
|---|---|
| `kind` | `sftp` |
| `host` | SFTP server hostname |
| `port` | SSH port (default: `22`) |
| `username` | SSH username |
| `password` | Password (or use `private_key_path`) |
| `remote_path` | Remote file path or glob pattern |

#### Streaming & Messaging

##### Kafka

| Field | Description |
|---|---|
| `kind` | `kafka` |
| `brokers` | Kafka bootstrap servers |
| `topic` | Topic to consume |
| `group_id` | Consumer group ID |

##### AMQP (RabbitMQ)

| Field | Description |
|---|---|
| `kind` | `amqp` |
| `connection_string` | AMQP connection URI |
| `queue` | Queue name |

##### SQS

| Field | Description |
|---|---|
| `kind` | `sqs` |
| `queue_url` | SQS queue URL |
| `region` | AWS region |

##### Kinesis

| Field | Description |
|---|---|
| `kind` | `kinesis` |
| `stream_name` | Kinesis stream name |
| `region` | AWS region |

##### SNS

| Field | Description |
|---|---|
| `kind` | `sns` |
| `topic_arn` | SNS topic ARN |
| `sqs_queue_url` | SQS queue URL subscribed to the topic |
| `region` | AWS region |

##### EventBridge

| Field | Description |
|---|---|
| `kind` | `eventbridge` |
| `event_bus_name` | EventBridge bus name |
| `sqs_queue_url` | SQS queue URL receiving events |
| `region` | AWS region |

##### MQTT

| Field | Description |
|---|---|
| `kind` | `mqtt` |
| `broker_url` | MQTT broker hostname |
| `port` | Broker port (default: `1883`) |
| `topic` | Topic to subscribe to |

##### WebSocket

| Field | Description |
|---|---|
| `kind` | `websocket` |
| `url` | WebSocket URL (`ws://` or `wss://`) |

#### Network & Other

##### HTTP Client

| Field | Description |
|---|---|
| `kind` | `http_client` |
| `url` | HTTP endpoint URL |
| `method` | HTTP method (default: `GET`) |
| `scrape_interval_seconds` | Polling interval (omit for one-shot) |

##### HTTP Server

| Field | Description |
|---|---|
| `kind` | `http_server` |
| `listen_address` | Address to bind (default: `0.0.0.0:8080`) |
| `path` | URL path to listen on |

##### Socket (TCP/UDP/Unix)

| Field | Description |
|---|---|
| `kind` | `socket` |
| `mode` | `tcp`, `udp`, or `unix` |
| `address` | Bind address (host:port or socket path) |

##### StatsD

| Field | Description |
|---|---|
| `kind` | `statsd` |
| `listen_address` | UDP address to listen on (default: `0.0.0.0:8125`) |

##### DynamoDB

| Field | Description |
|---|---|
| `kind` | `dynamodb` |
| `table_name` | DynamoDB table name |
| `region` | AWS region |

### dbt (optional)

Override dbt naming conventions.

| Field | Default | Description |
|---|---|---|
| `target_schema` | Same as `project` | Base schema name for dbt materialisation |
| `silver_suffix` | `silver` | Suffix for silver/staging tier |
| `gold_suffix` | `gold` | Suffix for gold/mart tier |

With `project: analytics`, dbt materialises to:

- `analytics_silver` (staging models)
- `analytics_gold` (mart models)
