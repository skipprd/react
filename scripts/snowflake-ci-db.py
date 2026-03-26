"""Create or drop an ephemeral Snowflake database for CI.

Usage:
    python scripts/snowflake-ci-db.py create <DB_NAME>
    python scripts/snowflake-ci-db.py drop   <DB_NAME>

Expects env vars: SNOWFLAKE_ACCOUNT, SNOWFLAKE_USER, SNOWFLAKE_PRIVATE_KEY_PATH
"""

import os
import sys

import snowflake.connector
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.backends import default_backend


def _connect():
    key_path = os.environ["SNOWFLAKE_PRIVATE_KEY_PATH"]
    with open(key_path, "rb") as f:
        p_key = serialization.load_pem_private_key(
            f.read(), password=None, backend=default_backend()
        )
    pkb = p_key.private_bytes(
        encoding=serialization.Encoding.DER,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption(),
    )
    return snowflake.connector.connect(
        account=os.environ["SNOWFLAKE_ACCOUNT"],
        user=os.environ["SNOWFLAKE_USER"],
        private_key=pkb,
        warehouse="COMPUTE_WH",
        role="ACCOUNTADMIN",
    )


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("create", "drop"):
        print(__doc__.strip())
        sys.exit(1)

    action, db_name = sys.argv[1], sys.argv[2]
    conn = _connect()
    cur = conn.cursor()
    try:
        if action == "create":
            cur.execute(f"CREATE DATABASE IF NOT EXISTS {db_name}")
            cur.execute(f"CREATE SCHEMA IF NOT EXISTS {db_name}.RAW")
            print(f"Created database {db_name} with schema RAW")
        else:
            cur.execute(f"DROP DATABASE IF EXISTS {db_name}")
            print(f"Dropped database {db_name}")
    finally:
        cur.close()
        conn.close()


if __name__ == "__main__":
    main()
