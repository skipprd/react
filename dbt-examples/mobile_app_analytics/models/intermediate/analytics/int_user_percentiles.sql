select
  percentile_cont(0.9) within group (order by total_events) as p90_events,
  percentile_cont(0.9) within group (order by avg_events_per_day) as p90_events_per_day,
  percentile_cont(0.9) within group (order by avg_sessions_per_day) as p90_sessions,
  percentile_cont(0.25) within group (order by avg_events_per_day) as p25_events_per_day,
  percentile_cont(0.25) within group (order by avg_sessions_per_day) as p25_sessions_per_day
from {{ ref('int_behavioral_features') }}
