

WITH source_data AS (
    SELECT
        BIKE_ID AS bike_id_raw,
        CRANK_TORQUES AS crank_torques,
        EVENT_DATE AS event_date_raw,
        EVENT_TYPE AS event_type_raw,
        HARDWARE AS hardware,
        ISBN AS isbn_raw,
        LAST_CRANK AS last_crank,
        MESSAGE_TYPE AS message_type_raw,
        METADATA AS metadata,
        RIDER_ID AS rider_id_raw,
        TRIP AS trip
    FROM ANALYTICS.RAW.TRIP_END
)
SELECT
    bike_id_raw,
    crank_torques,
    event_date_raw,
    event_type_raw,
    hardware,
    isbn_raw,
    last_crank,
    message_type_raw,
    metadata,
    rider_id_raw,
    trip,
    NULLIF(TRIM(bike_id_raw), '') AS bike_id,
    NULLIF(TRIM(rider_id_raw), '') AS rider_id,
    NULLIF(TRIM(isbn_raw), '') AS isbn,
    NULLIF(TRIM(event_type_raw), '') AS event_type,
    NULLIF(TRIM(message_type_raw), '') AS message_type,
    COALESCE(
        TRY_TO_TIMESTAMP_NTZ(event_date_raw),
        TRY_CAST(event_date_raw AS TIMESTAMP_NTZ)
    ) AS event_at,
    IFF(
        event_date_raw IS NOT NULL
        AND COALESCE(
            TRY_TO_TIMESTAMP_NTZ(event_date_raw),
            TRY_CAST(event_date_raw AS TIMESTAMP_NTZ)
        ) IS NULL,
        TRUE,
        FALSE
    ) AS event_at_parse_failed,
    'trip_end' AS event_name,
    'ANALYTICS.RAW.TRIP_END' AS source_relation
FROM source_data