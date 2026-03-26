{{ config(alias="fct_trip") }}

WITH trip_events AS (
    SELECT
        event_key,
        event_name,
        event_ts,
        event_date,
        bike_id,
        rider_id,
        trip_id
    FROM {{ ref('fct_trip_event') }}
),
trip_rollup AS (
    SELECT
        trip_id,
        MIN(event_ts) AS first_event_ts,
        MIN(event_date) AS first_event_date,
        MAX(event_ts) AS last_event_ts,
        COUNT(*) AS total_event_count,
        COUNT_IF(event_name = 'trip_pause') AS pause_event_count,
        COUNT_IF(event_name = 'trip_resume') AS resume_event_count,
        MIN(IFF(event_name = 'trip_start', event_ts, NULL)) AS trip_start_ts,
        MAX(IFF(event_name = 'trip_end', event_ts, NULL)) AS trip_end_ts
    FROM trip_events
    GROUP BY trip_id
),
start_anchor AS (
    SELECT
        trip_id,
        event_key AS anchor_start_event_key,
        bike_id AS start_bike_id,
        rider_id AS start_rider_id
    FROM trip_events
    WHERE event_name = 'trip_start'
    QUALIFY ROW_NUMBER() OVER (
        PARTITION BY trip_id
        ORDER BY event_ts ASC, event_key ASC
    ) = 1
),
end_anchor AS (
    SELECT
        trip_id,
        event_key AS anchor_end_event_key,
        bike_id AS end_bike_id,
        rider_id AS end_rider_id
    FROM trip_events
    WHERE event_name = 'trip_end'
    QUALIFY ROW_NUMBER() OVER (
        PARTITION BY trip_id
        ORDER BY event_ts DESC, event_key DESC
    ) = 1
),
latest_trip_values AS (
    SELECT
        trip_id,
        bike_id AS latest_bike_id,
        rider_id AS latest_rider_id
    FROM trip_events
    QUALIFY ROW_NUMBER() OVER (
        PARTITION BY trip_id
        ORDER BY event_ts DESC, event_key DESC
    ) = 1
)
SELECT
    MD5(COALESCE(tr.trip_id, '')) AS trip_key,
    'trip_id' AS trip_grain_type,
    tr.trip_id,
    COALESCE(sa.start_bike_id, ea.end_bike_id, lv.latest_bike_id) AS bike_id,
    COALESCE(sa.start_rider_id, ea.end_rider_id, lv.latest_rider_id) AS rider_id,
    COALESCE(tr.trip_start_ts, tr.first_event_ts) AS trip_start_ts,
    tr.trip_end_ts,
    COALESCE(CAST(tr.trip_end_ts AS DATE), CAST(tr.trip_start_ts AS DATE), tr.first_event_date) AS trip_activity_date,
    CASE
        WHEN tr.trip_end_ts IS NOT NULL THEN 'completed'
        ELSE 'in_progress'
    END AS trip_status,
    tr.pause_event_count,
    tr.resume_event_count,
    tr.total_event_count,
    CASE
        WHEN tr.trip_start_ts IS NOT NULL
         AND tr.trip_end_ts IS NOT NULL
         AND tr.trip_end_ts >= tr.trip_start_ts
            THEN DATEDIFF('second', tr.trip_start_ts, tr.trip_end_ts)
        ELSE NULL
    END AS trip_duration_seconds,
    sa.anchor_start_event_key,
    ea.anchor_end_event_key,
    CASE
        WHEN tr.trip_start_ts IS NULL AND tr.trip_end_ts IS NULL THEN 'missing_start_and_end'
        WHEN tr.trip_start_ts IS NULL THEN 'missing_start_event'
        WHEN tr.trip_end_ts IS NULL THEN 'missing_end_event'
        WHEN tr.trip_end_ts < tr.trip_start_ts THEN 'end_before_start'
        WHEN COALESCE(sa.start_bike_id, ea.end_bike_id, lv.latest_bike_id) IS NULL THEN 'missing_bike_id'
        WHEN COALESCE(sa.start_rider_id, ea.end_rider_id, lv.latest_rider_id) IS NULL THEN 'missing_rider_id'
        ELSE 'ok'
    END AS data_quality_flag
FROM trip_rollup tr
LEFT JOIN start_anchor sa
    ON tr.trip_id = sa.trip_id
LEFT JOIN end_anchor ea
    ON tr.trip_id = ea.trip_id
LEFT JOIN latest_trip_values lv
    ON tr.trip_id = lv.trip_id