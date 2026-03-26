

WITH rider_events AS (
    SELECT
        rider_id,
        event_ts,
        event_date
    FROM ANALYTICS.bike_hire_gold.fct_trip_event
    WHERE rider_id IS NOT NULL
),
rider_first_last AS (
    SELECT
        rider_id,
        MIN(event_ts) AS first_seen_ts,
        MAX(event_ts) AS last_seen_ts,
        MIN(event_date) AS first_seen_date,
        MAX(event_date) AS last_seen_date
    FROM rider_events
    GROUP BY rider_id
)
SELECT
    MD5(COALESCE(rider_id, '')) AS rider_key,
    rider_id,
    first_seen_ts,
    last_seen_ts,
    first_seen_date,
    last_seen_date
FROM rider_first_last