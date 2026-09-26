# Object storage (Cloudflare R2)

Normative. Locked by [decisions.md](./decisions.md) **R7**.

Skippr ReAct durable object storage (threads, artifacts, Lance vectors) **MUST** be **Cloudflare R2** via the S3-compatible API. **MUST NOT** use AWS S3, STS, IAM, or the default AWS credential chain.

Auth is the same platform R2 keys Skippr Cloud already uses (`OBJECTS_*` in [cloudflare.md](../../cloud/specs/cloudflare.md) §3). Tenants MUST NOT need a second object-store account.

## Buckets (Terraform)

Ops Terraform: [`infra/cloudflare/r2/`](../infra/cloudflare/r2/).

| Bucket | Role |
|--------|------|
| `skippr-react-runtime` | Threads, plans, control state, run logs, artifacts |
| `skippr-react-vectors` | LanceDB embeddings / vector indices |

Buckets are **private**. **MUST NOT** attach a Worker, public custom domain, or CDN to them.

Create:

```bash
cd infra/cloudflare/r2
set -a && source ../../../cloud/.env && set +a   # or any env with CLOUDFLARE_* 
terraform init
terraform apply \
  -var="cloudflare_account_id=$CLOUDFLARE_ACCOUNT_ID" \
  -var="cloudflare_api_token=$CLOUDFLARE_API_TOKEN"
```

If the existing `OBJECTS_*` S3 API token is account-scoped, new buckets are usable immediately. If it is bucket-scoped, add these two names to the token.

## Runtime wiring

| Env | Purpose |
|-----|---------|
| `OBJECTS_S3_ENDPOINT` | `https://{ACCOUNT_ID}.r2.cloudflarestorage.com` |
| `OBJECTS_ACCESS_KEY_ID` | R2 S3 API key |
| `OBJECTS_SECRET_ACCESS_KEY` | R2 S3 API secret |
| `OBJECTS_S3_REGION` | `auto` (default) |
| `OBJECTS_BUCKET_REACT_RUNTIME` | Override runtime bucket (default `skippr-react-runtime`) |
| `OBJECTS_BUCKET_REACT_VECTORS` | Override vectors bucket (default `skippr-react-vectors`) |

YAML:

```yaml
storage:
  mode: r2
  bucket: skippr-react-runtime
  vectors_bucket: skippr-react-vectors
```

`storage.mode: local` remains valid for laptop-only runs (no R2).

## Adapter rules

- S3 SDK **MAY** be used as an S3-compatible client. It **MUST** set `endpoint_url` to R2, `region` `auto`, and **path-style** addressing.
- Lance `storage_options` **MUST** include `aws_endpoint` and `aws_virtual_hosted_style_request=false`.
- **MUST NOT** call `aws_config::load_from_env` / default AWS chain as the object-store credential source.
- **MUST NOT** mint AWS STS for ReAct storage.

## Not ReAct infra

Warehouse engines (Athena result locations, Glue, Snowflake stages) are **product-host** (`sde`) concerns. They are not created by this Terraform. Athena still requires AWS if a host uses Athena; that does not belong in `react`.
