
  create or replace   view ANALYTICS.bike_hire_silver.stg_raw_trip_pause
  
  
  
  
  as (
    

WITH staged AS (
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
        TRIP AS trip,
        ISBN_STRING AS isbn_string_raw,

        NULLIF(TRIM(BIKE_ID), '') AS bike_id,
        NULLIF(TRIM(RIDER_ID), '') AS rider_id,
        COALESCE(NULLIF(TRIM(ISBN_STRING), ''), TO_VARCHAR(ISBN)) AS isbn,
        NULLIF(TRIM(EVENT_TYPE), '') AS event_type,
        NULLIF(TRIM(MESSAGE_TYPE), '') AS message_type,
        COALESCE(
            TRY_TO_TIMESTAMP_NTZ(EVENT_DATE),
            TRY_CAST(EVENT_DATE AS TIMESTAMP_NTZ)
        ) AS event_at,

        'trip_pause' AS event_name,
        'ANALYTICS.RAW.TRIP_PAUSE' AS source_relation,

        IFF(
            EVENT_DATE IS NOT NULL
            AND COALESCE(
                TRY_TO_TIMESTAMP_NTZ(EVENT_DATE),
                TRY_CAST(EVENT_DATE AS TIMESTAMP_NTZ)
            ) IS NULL,
            TRUE,
            FALSE
        ) AS event_at_parse_failed,
        IFF(NULLIF(TRIM(ISBN_STRING), '') IS NOT NULL, TRUE, FALSE) AS isbn_used_isbn_string,
        IFF(NULLIF(TRIM(ISBN_STRING), '') IS NULL AND ISBN IS NOT NULL, TRUE, FALSE) AS isbn_used_numeric_fallback
    FROM ANALYTICS.RAW.TRIP_PAUSE
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
    isbn_string_raw,
    bike_id,
    rider_id,
    isbn,
    event_type,
    message_type,
    event_at,
    event_name,
    source_relation,
    event_at_parse_failed
FROM staged
  );

