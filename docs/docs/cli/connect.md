# skippr connect

Configure a warehouse or data source connection.

## Usage

```bash
skippr connect warehouse <kind> [flags]
skippr connect source <kind> [flags]
```

When flags are omitted, the command prompts interactively.

---

## connect warehouse

### Athena (S3 + Glue)

```bash
skippr connect warehouse athena \
  --workgroup primary \
  --region us-east-1 \
  --result-s3 s3://my-bucket/athena-results/ \
  --catalog AwsDataCatalog \
  --schema my_database
```

| Flag | Description |
|---|---|
| `--workgroup` | Athena workgroup name (default: primary) |
| `--region` | AWS region |
| `--result-s3` | S3 output location for query results (omit to use workgroup default) |
| `--catalog` | Glue Data Catalog name (default: `AwsDataCatalog`) |
| `--schema` | Default database/schema for discovery and unqualified queries |

Authentication uses the AWS default credential chain (env vars, instance profile, SSO).

### Snowflake

```bash
skippr connect warehouse snowflake \
  --database ANALYTICS \
  --schema RAW \
  --warehouse COMPUTE_WH \
  --role ACCOUNTADMIN
```

| Flag | Description |
|---|---|
| `--database` | Snowflake database name |
| `--schema` | Bronze/raw schema where extracted data lands |
| `--warehouse` | Compute warehouse |
| `--role` | Role with appropriate grants |

Authentication is always via environment variables (`SNOWFLAKE_ACCOUNT`, `SNOWFLAKE_USER`, `SNOWFLAKE_PRIVATE_KEY_PATH` or `SNOWFLAKE_PASSWORD`). These are never stored in the config file.

### BigQuery

```bash
skippr connect warehouse bigquery \
  --project my-gcp-project \
  --dataset raw_data \
  --location US
```

| Flag | Description |
|---|---|
| `--project` | GCP project ID |
| `--dataset` | BigQuery dataset |
| `--location` | Dataset location (e.g. `US`, `EU`) |

### Postgres

```bash
skippr connect warehouse postgres \
  --database analytics \
  --schema public
```

| Flag | Description |
|---|---|
| `--database` | PostgreSQL database name |
| `--schema` | Target schema (default: `public`) |

Authentication is via environment variables (`POSTGRES_HOST`, `POSTGRES_USER`, `POSTGRES_PASSWORD`). See [Environment Variables](../configuration/environment-variables.md).

### Databricks

```bash
skippr connect warehouse databricks \
  --workspace-url https://dbc-xxxxxxxx.cloud.databricks.com \
  --token "dapi..." \
  --warehouse-id abc123 \
  --catalog main \
  --schema default
```

| Flag | Description |
|---|---|
| `--workspace-url` | Databricks workspace URL |
| `--token` | Personal access token |
| `--warehouse-id` | SQL warehouse ID |
| `--catalog` | Unity Catalog name (default: `main`) |
| `--schema` | Target schema for bronze/raw data (default: `default`) |

### Azure Synapse

```bash
skippr connect warehouse synapse \
  --connection-string "Server=myserver.database.windows.net;User Id=admin;Password=secret;Database=mydb" \
  --schema dbo
```

| Flag | Description |
|---|---|
| `--connection-string` | ADO.NET connection string |
| `--schema` | Target schema (default: `dbo`) |

### Amazon Redshift

```bash
skippr connect warehouse redshift \
  --database analytics \
  --cluster-identifier my-cluster \
  --db-user admin \
  --schema public \
  --region us-east-1
```

| Flag | Description |
|---|---|
| `--database` | Redshift database name |
| `--cluster-identifier` | Provisioned cluster ID (use this or `--workgroup-name`) |
| `--workgroup-name` | Serverless workgroup name (use this or `--cluster-identifier`) |
| `--db-user` | Database user (provisioned clusters) |
| `--schema` | Target schema (default: `public`) |
| `--region` | AWS region |

Authentication uses the AWS default credential chain (env vars, instance profile, SSO).

### ClickHouse

```bash
skippr connect warehouse clickhouse \
  --url http://localhost:8123 \
  --database default \
  --user default \
  --password "secret"
```

| Flag | Description |
|---|---|
| `--url` | HTTP interface URL (default: `http://localhost:8123`) |
| `--database` | Database name (default: `default`) |
| `--user` | Username (default: `default`) |
| `--password` | Password |

### MotherDuck

```bash
skippr connect warehouse motherduck \
  --motherduck-token "md:..." \
  --database my_database \
  --schema main
```

| Flag | Description |
|---|---|
| `--motherduck-token` | MotherDuck auth token (required) |
| `--database` | MotherDuck database name |
| `--schema` | Target schema (default: `main`) |

---

## connect source

### MSSQL

```bash
skippr connect source mssql \
  --connection-string '${MSSQL_CONNECTION_STRING}'
```

| Flag | Description |
|---|---|
| `--connection-string` | ADO.NET connection string. For security best practices, we strongly advise against storing the connection string in `skippr.yaml`. Use environment variable interpolation instead: replace the `connection_string` value with your own `${ENV_VAR}` reference. |

This writes the following to `skippr.yaml`:

```yaml
source:
  kind: mssql
  connection_string: ${MSSQL_CONNECTION_STRING}
```

Set the env var before running `skippr`:

macOS / Linux

```bash
export MSSQL_CONNECTION_STRING="server=tcp:127.0.0.1,1433;database=testdb;user id=sa;password=YourPass;TrustServerCertificate=true"
```

Windows PowerShell

```powershell
$env:MSSQL_CONNECTION_STRING = "server=tcp:127.0.0.1,1433;database=testdb;user id=sa;password=YourPass;TrustServerCertificate=true"
```

Windows Command Prompt

```cmd
set MSSQL_CONNECTION_STRING=server=tcp:127.0.0.1,1433;database=testdb;user id=sa;password=YourPass;TrustServerCertificate=true
```

### MySQL

```bash
skippr connect source mysql \
  --connection-string 'mysql://user:pass@host:3306/db' \
  --tables customers,orders
```

| Flag | Description |
|---|---|
| `--connection-string` | MySQL connection string |
| `--tables` | Comma-separated list of tables (omit to discover all) |

### PostgreSQL (source)

```bash
skippr connect source postgres-source \
  --host localhost --port 5432 \
  --user myuser --password mypass \
  --database mydb \
  --tables public.customers,public.orders
```

| Flag | Description |
|---|---|
| `--host` | Postgres host |
| `--port` | Postgres port (default: `5432`) |
| `--user` | Username |
| `--password` | Password |
| `--database` | Database name |
| `--connection-string` | Full connection string (overrides individual fields) |
| `--tables` | Comma-separated list of tables |
| `--query` | SQL query instead of table list |

### Amazon Redshift (source)

```bash
skippr connect source redshift-source \
  --cluster-identifier my-cluster \
  --database analytics \
  --db-user admin \
  --tables public.customers,public.orders \
  --region us-east-1
```

| Flag | Description |
|---|---|
| `--cluster-identifier` | Redshift cluster ID (or use `--workgroup-name` for Serverless) |
| `--workgroup-name` | Serverless workgroup name |
| `--database` | Database name |
| `--db-user` | Database user |
| `--tables` | Comma-separated list of tables |
| `--region` | AWS region |

### MongoDB

```bash
skippr connect source mongodb \
  --connection-string 'mongodb://user:pass@host:27017' \
  --database mydb \
  --collection users
```

| Flag | Description |
|---|---|
| `--connection-string` | MongoDB connection URI |
| `--database` | Database name |
| `--collection` | Collection name |
| `--filter` | Optional JSON filter document |

### DynamoDB

```bash
skippr connect source dynamodb \
  --table-name my-table \
  --region us-east-1
```

| Flag | Description |
|---|---|
| `--table-name` | DynamoDB table name |
| `--region` | AWS region |
| `--endpoint-url` | Optional custom endpoint URL |

### ClickHouse (source)

```bash
skippr connect source clickhouse-source \
  --url http://localhost:8123 \
  --database default \
  --user default \
  --tables events,metrics
```

| Flag | Description |
|---|---|
| `--url` | HTTP interface URL |
| `--database` | Database name |
| `--user` | Username |
| `--password` | Password |
| `--tables` | Comma-separated list of tables |
| `--query` | SQL query instead of table list |

### MotherDuck (source)

```bash
skippr connect source motherduck-source \
  --motherduck-token "$MOTHERDUCK_TOKEN" \
  --database my_db \
  --tables main.customers,main.orders
```

| Flag | Description |
|---|---|
| `--motherduck-token` | MotherDuck auth token (required, or set `MOTHERDUCK_TOKEN`) |
| `--database` | MotherDuck database name |
| `--tables` | Comma-separated list of tables |
| `--query` | SQL query instead of table list |

### S3

```bash
skippr connect source s3 \
  --bucket my-data-bucket \
  --prefix raw/
```

| Flag | Description |
|---|---|
| `--bucket` | S3 bucket name |
| `--prefix` | Key prefix to scan |
| `--namespace-fields` | Field(s) used to namespace incoming events |

### SFTP

```bash
skippr connect source sftp \
  --host sftp.example.com \
  --username myuser \
  --private-key-path ~/.ssh/id_rsa \
  --remote-path /data/exports/
```

| Flag | Description |
|---|---|
| `--host` | SFTP server hostname |
| `--port` | SSH port (default: `22`) |
| `--username` | SSH username |
| `--password` | Password (or use `--private-key-path`) |
| `--private-key-path` | Path to SSH private key |
| `--remote-path` | Remote file path or glob pattern |

### Delta Lake

```bash
skippr connect source delta-lake \
  --table-uri s3://bucket/path/to/table
```

| Flag | Description |
|---|---|
| `--table-uri` | Table location URI (e.g. `s3://bucket/path`) |
| `--filter` | Optional filter expression |

### Local File

```bash
skippr connect source file \
  --path /data/export.csv
```

| Flag | Description |
|---|---|
| `--path` | Path to local file |

### Kafka

```bash
skippr connect source kafka \
  --brokers localhost:9092 \
  --topic events \
  --group-id skippr-consumer
```

| Flag | Description |
|---|---|
| `--brokers` | Kafka bootstrap servers |
| `--topic` | Topic to consume |
| `--group-id` | Consumer group ID |
| `--mode` | Consumption mode |

### SQS

```bash
skippr connect source sqs \
  --queue-url https://sqs.us-east-1.amazonaws.com/123/my-queue \
  --region us-east-1
```

| Flag | Description |
|---|---|
| `--queue-url` | SQS queue URL |
| `--region` | AWS region |
| `--mode` | Consumption mode |

### Kinesis

```bash
skippr connect source kinesis \
  --stream-name my-stream \
  --region us-east-1
```

| Flag | Description |
|---|---|
| `--stream-name` | Kinesis stream name |
| `--region` | AWS region |
| `--mode` | Consumption mode |

### AMQP (RabbitMQ)

```bash
skippr connect source amqp \
  --connection-string 'amqp://guest:guest@localhost:5672' \
  --queue my-queue
```

| Flag | Description |
|---|---|
| `--connection-string` | AMQP connection URI |
| `--queue` | Queue name |
| `--mode` | Consumption mode |

### SNS

```bash
skippr connect source sns \
  --topic-arn arn:aws:sns:us-east-1:123:my-topic \
  --sqs-queue-url https://sqs.us-east-1.amazonaws.com/123/sns-queue \
  --region us-east-1
```

| Flag | Description |
|---|---|
| `--topic-arn` | SNS topic ARN |
| `--sqs-queue-url` | SQS queue URL subscribed to the topic |
| `--region` | AWS region |

### EventBridge

```bash
skippr connect source eventbridge \
  --event-bus-name my-bus \
  --sqs-queue-url https://sqs.us-east-1.amazonaws.com/123/eb-queue \
  --region us-east-1
```

| Flag | Description |
|---|---|
| `--event-bus-name` | EventBridge bus name |
| `--sqs-queue-url` | SQS queue URL receiving events |
| `--region` | AWS region |

### MQTT

```bash
skippr connect source mqtt \
  --broker-url mqtt://broker.example.com \
  --topic sensors/temperature
```

| Flag | Description |
|---|---|
| `--broker-url` | MQTT broker hostname |
| `--topic` | Topic to subscribe to |
| `--mode` | Consumption mode |

### WebSocket

```bash
skippr connect source websocket \
  --url wss://stream.example.com/v1
```

| Flag | Description |
|---|---|
| `--url` | WebSocket URL (`ws://` or `wss://`) |
| `--mode` | Consumption mode |

### Google Analytics (GA4)

```bash
skippr connect source google-analytics \
  --property-id 123456789 \
  --start-date 2024-01-01 \
  --stream-profile full \
  --lookback-days 7 \
  --processing-lag-days 1 \
  --window-in-days 1 \
  --access-token ${GA4_ACCESS_TOKEN}
```

| Flag | Description |
|---|---|
| `--property-id` | GA4 property ID (numeric) |
| `--start-date` | First sync date (`YYYY-MM-DD`) |
| `--end-date` | Last sync date (optional) |
| `--stream-profile` | `minimal`, `standard`, or `full` (default in plugin: `full`, 23 namespaces) |
| `--lookback-days` | Days before checkpoint to re-fetch (default: 3) |
| `--processing-lag-days` | Skip last N calendar days (default: 1) |
| `--window-in-days` | Days per API date range (default: 1; >1 may sample) |
| `--keep-empty-rows` | Include zero-metric dimension rows (default: true) |
| `--access-token` | Bearer token or `${GA4_ACCESS_TOKEN}` |
| `--oauth-token-url` | OAuth token URL (refresh flow) |
| `--oauth-client-id` | OAuth client ID |
| `--oauth-client-secret` | OAuth client secret |
| `--oauth-refresh-token` | OAuth refresh token |
| `--service-account-json-path` | Service account JSON key path |
| `--streams` | Comma-separated namespaces (overrides profile) |

Writes `GoogleAnalytics` under `data_sources` in engine `skippr.yml`, or `source.kind: google_analytics` in public `skippr.yaml`. Pair with Athena or Iceberg for `replace_partition` landing.

**Auth setup (public docs):** [Service account](https://docs.skippr.io/connectors/sources/google-analytics#service-account) · [OAuth refresh](https://docs.skippr.io/connectors/sources/google-analytics#oauth-refresh) · [Bearer token (`GA4_ACCESS_TOKEN`)](https://docs.skippr.io/connectors/sources/google-analytics#bearer-access-token-ga4_access_token)

### HTTP Client

```bash
skippr connect source http-client \
  --url https://api.example.com/data \
  --method GET \
  --scrape-interval-seconds 60
```

| Flag | Description |
|---|---|
| `--url` | HTTP endpoint URL |
| `--method` | HTTP method (default: `GET`) |
| `--scrape-interval-seconds` | Polling interval in seconds (omit for one-shot) |

### HTTP Server

```bash
skippr connect source http-server \
  --listen-address 0.0.0.0:8080 \
  --path /webhook
```

| Flag | Description |
|---|---|
| `--listen-address` | Address to bind (default: `0.0.0.0:8080`) |
| `--path` | URL path to listen on |

### Socket (TCP/UDP/Unix)

```bash
skippr connect source socket \
  --mode tcp \
  --address 0.0.0.0:9000
```

| Flag | Description |
|---|---|
| `--mode` | `tcp`, `udp`, or `unix` |
| `--address` | Bind address (host:port or socket path) |

### StatsD

```bash
skippr connect source statsd \
  --listen-address 0.0.0.0:8125
```

| Flag | Description |
|---|---|
| `--listen-address` | UDP address to listen on (default: `0.0.0.0:8125`) |

### Stdin

```bash
skippr connect source stdin \
  --mode json
```

| Flag | Description |
|---|---|
| `--mode` | Input format mode |

---

## Notes

- `connect` commands update `skippr.yaml` in the current directory. Run `skippr init` first.
- Sensitive values (passwords, keys, connection strings) should use environment variable references in the config and be set in your shell or `.env` file.
