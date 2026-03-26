

WITH staged AS (
    SELECT
        BIKE_ID AS bike_id_raw,
        CRANK_TORQUES AS crank_torques_raw,
        EVENT_DATE AS event_date_raw,
        EVENT_TYPE AS event_type_raw,
        HARDWARE AS hardware_raw,
        ISBN AS isbn_raw,
        LAST_CRANK AS last_crank_raw,
        MESSAGE_TYPE AS message_type_raw,
        METADATA AS metadata_raw,
        RIDER_ID AS rider_id_raw,
        TRIP AS trip_raw,

        NULLIF(TRIM(BIKE_ID), '') AS bike_id,
        NULLIF(TRIM(RIDER_ID), '') AS rider_id,
        NULLIF(TRIM(ISBN), '') AS isbn,
        NULLIF(TRIM(EVENT_TYPE), '') AS event_type,
        NULLIF(TRIM(MESSAGE_TYPE), '') AS message_type,
        TRY_TO_TIMESTAMP_NTZ(EVENT_DATE) AS event_at,

        CRANK_TORQUES AS crank_torques,
        HARDWARE AS hardware,
        LAST_CRANK AS last_crank,
        METADATA AS metadata,
        TRIP AS trip,

        'trip_start' AS event_name,
        'ANALYTICS.RAW.TRIP_START' AS source_relation,

        IFF(
            EVENT_DATE IS NOT NULL
            AND TRY_TO_TIMESTAMP_NTZ(EVENT_DATE) IS NULL,
            TRUE,
            FALSE
        ) AS event_at_parse_failed
    FROM ANALYTICS.RAW.TRIP_START
)
SELECT
    bike_id_raw,
    crank_torques_raw,
    event_date_raw,
    event_type_raw,
    hardware_raw,
    isbn_raw,
    last_crank_raw,
    message_type_raw,
    metadata_raw,
    rider_id_raw,
    trip_raw,
    event_name,
    source_relation,
    bike_id,
    rider_id,
    isbn,
    event_type,
    message_type,
    event_at,
    crank_torques,
    hardware,
    last_crank,
    metadata,
    trip,
    event_at_parse_failed
FROM staged