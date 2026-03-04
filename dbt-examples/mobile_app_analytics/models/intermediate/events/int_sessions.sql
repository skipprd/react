{{ sessionize(table=ref('stg_events'), user_col='user_id', timestamp_col='event_ts', session_gap_minutes=30) }}
