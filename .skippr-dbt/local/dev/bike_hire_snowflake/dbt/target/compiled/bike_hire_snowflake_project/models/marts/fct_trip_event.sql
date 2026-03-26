

WITH trip_start_events AS (
    SELECT
        'stg_raw_trip_start' AS source_event_model,
        'start' AS event_name,
        event_at AS event_ts,
        CAST(event_at AS DATE) AS event_date,
        bike_id,
        rider_id,
        isbn,
        message_type,
        CAST(trip AS TEXT) AS trip_id,
        event_type AS event_type_raw,
        IFF(
            bike_id IS NOT NULL
            OR rider_id IS NOT NULL
            OR isbn IS NOT NULL
            OR message_type IS NOT NULL
            OR event_type IS NOT NULL
            OR trip IS NOT NULL,
            TRUE,
            FALSE
        ) AS event_payload_present_flag
    FROM ANALYTICS.bike_hire_silver.stg_raw_trip_start
    WHERE event_at IS NOT NULL
),
trip_pause_events AS (
    SELECT
        'stg_raw_trip_pause' AS source_event_model,
        'pause' AS event_name,
        event_at AS event_ts,
        CAST(event_at AS DATE) AS event_date,
        bike_id,
        rider_id,
        isbn,
        message_type,
        CAST(trip AS TEXT) AS trip_id,
        event_type AS event_type_raw,
        IFF(
            bike_id IS NOT NULL
            OR rider_id IS NOT NULL
            OR isbn IS NOT NULL
            OR message_type IS NOT NULL
            OR event_type IS NOT NULL
            OR trip IS NOT NULL,
            TRUE,
            FALSE
        ) AS event_payload_present_flag
    FROM ANALYTICS.bike_hire_silver.stg_raw_trip_pause
    WHERE event_at IS NOT NULL
),
trip_resume_events AS (
    SELECT
        'stg_raw_trip_resume' AS source_event_model,
        'resume' AS event_name,
        event_at AS event_ts,
        CAST(event_at AS DATE) AS event_date,
        bike_id,
        rider_id,
        isbn,
        message_type,
        CAST(trip AS TEXT) AS trip_id,
        event_type AS event_type_raw,
        IFF(
            bike_id IS NOT NULL
            OR rider_id IS NOT NULL
            OR isbn IS NOT NULL
            OR message_type IS NOT NULL
            OR event_type IS NOT NULL
            OR trip IS NOT NULL,
            TRUE,
            FALSE
        ) AS event_payload_present_flag
    FROM ANALYTICS.bike_hire_silver.stg_raw_trip_resume
    WHERE event_at IS NOT NULL
),
trip_end_events AS (
    SELECT
        'stg_raw_trip_end' AS source_event_model,
        'end' AS event_name,
        event_at AS event_ts,
        CAST(event_at AS DATE) AS event_date,
        bike_id,
        rider_id,
        isbn,
        message_type,
        CAST(trip AS TEXT) AS trip_id,
        event_type AS event_type_raw,
        IFF(
            bike_id IS NOT NULL
            OR rider_id IS NOT NULL
            OR isbn IS NOT NULL
            OR message_type IS NOT NULL
            OR event_type IS NOT NULL
            OR trip IS NOT NULL,
            TRUE,
            FALSE
        ) AS event_payload_present_flag
    FROM ANALYTICS.bike_hire_silver.stg_raw_trip_end
    WHERE event_at IS NOT NULL
),
unified_events AS (
    SELECT * FROM trip_start_events
    UNION ALL
    SELECT * FROM trip_pause_events
    UNION ALL
    SELECT * FROM trip_resume_events
    UNION ALL
    SELECT * FROM trip_end_events
)
SELECT
    MD5(
        CONCAT_WS(
            '||',
            COALESCE(source_event_model, ''),
            COALESCE(event_name, ''),
            COALESCE(TO_VARCHAR(event_ts, 'YYYY-MM-DD HH24:MI:SS.FF9'), ''),
            COALESCE(event_type_raw, ''),
            COALESCE(message_type, ''),
            COALESCE(bike_id, ''),
            COALESCE(rider_id, ''),
            COALESCE(isbn, ''),
            COALESCE(trip_id, '')
        )
    ) AS event_key,
    event_name,
    event_ts,
    event_date,
    bike_id,
    rider_id,
    isbn,
    message_type,
    trip_id,
    source_event_model,
    event_type_raw,
    event_payload_present_flag
FROM unified_events