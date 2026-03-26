
  create or replace   view ANALYTICS.bike_hire_gold.agg_rider_daily_trip_activity
  
  
  
  
  as (
    

WITH rider_trips AS (
    SELECT
        rider_id,
        trip_activity_date,
        bike_id,
        trip_status,
        data_quality_flag,
        trip_duration_seconds
    FROM ANALYTICS.bike_hire_gold.fct_trip
    WHERE rider_id IS NOT NULL
      AND trip_activity_date IS NOT NULL
)
SELECT
    rider_id,
    trip_activity_date AS activity_date,
    COUNT(*) AS trip_count,
    COUNT_IF(trip_status = 'completed') AS completed_trip_count,
    COUNT_IF(trip_status = 'in_progress') AS in_progress_trip_count,
    COUNT_IF(data_quality_flag <> 'ok') AS anomaly_trip_count,
    COUNT(DISTINCT bike_id) AS distinct_bike_count,
    SUM(trip_duration_seconds) AS total_trip_duration_seconds,
    AVG(trip_duration_seconds) AS avg_trip_duration_seconds
FROM rider_trips
GROUP BY
    rider_id,
    trip_activity_date
  );

