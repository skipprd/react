{{ config(alias="stg_raw_trip_resume") }}

WITH staged AS (
    SELECT
        BIKE_ID AS bike_id_raw,
        CRANK_TORQUES AS crank_torques,
        EVENT_DATE AS event_date_source,
        EVENT_TYPE AS event_type_raw,
        HARDWARE AS hardware,
        ISBN AS isbn_raw,
        LAST_CRANK AS last_crank,
        MESSAGE_TYPE AS message_type_raw,
        METADATA AS metadata,
        RIDER_ID AS rider_id_raw,
        TRIP AS trip,
        NULLIF(TRIM(BIKE_ID), '') AS bike_id,
        NULLIF(TRIM(RIDER_ID), '') AS rider_id,
        NULLIF(TRIM(ISBN), '') AS isbn,
        NULLIF(TRIM(EVENT_TYPE), '') AS event_type,
        NULLIF(TRIM(MESSAGE_TYPE), '') AS message_type,
        EVENT_DATE AS event_date_raw,
        COALESCE(
            TRY_TO_TIMESTAMP_NTZ(EVENT_DATE),
            TRY_CAST(EVENT_DATE AS TIMESTAMP_NTZ)
        ) AS event_at,
        IFF(
            EVENT_DATE IS NOT NULL
            AND COALESCE(
                TRY_TO_TIMESTAMP_NTZ(EVENT_DATE),
                TRY_CAST(EVENT_DATE AS TIMESTAMP_NTZ)
            ) IS NULL,
            TRUE,
            FALSE
        ) AS event_at_parse_failed,
        'trip_resume' AS event_name,
        'ANALYTICS.RAW.TRIP_RESUME' AS source_relation
    FROM {{ source("RAW", "TRIP_RESUME") }}
)
SELECT
    bike_id_raw,
    crank_torques,
    event_date_source,
    event_type_raw,
    hardware,
    isbn_raw,
    last_crank,
    message_type_raw,
    metadata,
    rider_id_raw,
    trip,
    bike_id,
    rider_id,
    isbn,
    event_type,
    message_type,
    event_date_raw,
    event_at,
    event_at_parse_failed,
    event_name,
    source_relation
FROM staged