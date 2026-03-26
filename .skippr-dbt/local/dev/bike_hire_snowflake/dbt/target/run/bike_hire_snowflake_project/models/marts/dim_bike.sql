
  create or replace   view ANALYTICS.bike_hire_gold.dim_bike
  
  
  
  
  as (
    

WITH bike_events AS (
    SELECT
        event_key,
        event_ts,
        event_date,
        bike_id,
        isbn,
        message_type
    FROM ANALYTICS.bike_hire_gold.fct_trip_event
    WHERE bike_id IS NOT NULL
),
bike_rollup AS (
    SELECT
        bike_id,
        MIN(event_ts) AS first_seen_ts,
        MAX(event_ts) AS last_seen_ts,
        MIN(event_date) AS first_seen_date,
        MAX(event_date) AS last_seen_date
    FROM bike_events
    GROUP BY bike_id
),
latest_bike_event AS (
    SELECT
        bike_id,
        isbn AS latest_isbn,
        message_type AS latest_message_type
    FROM bike_events
    QUALIFY ROW_NUMBER() OVER (
        PARTITION BY bike_id
        ORDER BY event_ts DESC, event_key DESC
    ) = 1
)
SELECT
    MD5(br.bike_id) AS bike_key,
    br.bike_id,
    br.first_seen_ts,
    br.last_seen_ts,
    br.first_seen_date,
    br.last_seen_date,
    lbe.latest_isbn,
    lbe.latest_message_type
FROM bike_rollup br
LEFT JOIN latest_bike_event lbe
    ON br.bike_id = lbe.bike_id
  );

