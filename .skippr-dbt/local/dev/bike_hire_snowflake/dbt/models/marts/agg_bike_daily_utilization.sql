{{ config(alias="agg_bike_daily_utilization") }}

WITH bike_trips AS (
    SELECT
        bike_id,
        trip_activity_date,
        rider_id,
        trip_status,
        data_quality_flag,
        trip_duration_seconds
    FROM {{ ref('fct_trip') }}
    WHERE bike_id IS NOT NULL
      AND trip_activity_date IS NOT NULL
)
SELECT
    bike_id,
    trip_activity_date AS activity_date,
    COUNT(*) AS trip_count,
    COUNT(DISTINCT rider_id) AS distinct_rider_count,
    COUNT_IF(trip_status = 'completed') AS completed_trip_count,
    COUNT_IF(trip_status = 'in_progress') AS in_progress_trip_count,
    COUNT_IF(data_quality_flag <> 'ok') AS anomaly_trip_count,
    SUM(trip_duration_seconds) AS total_trip_duration_seconds,
    AVG(trip_duration_seconds) AS avg_trip_duration_seconds
FROM bike_trips
GROUP BY
    bike_id,
    trip_activity_date