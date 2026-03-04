with f as (
  select * from {{ ref('int_behavioral_features') }}
),
p as (
  select * from {{ ref('int_user_percentiles') }}
)
select
  f.user_id,
  f.total_events,
  f.avg_events_per_day,
  f.avg_sessions_per_day,
  case
    when f.total_events >= p.p90_events
      or f.avg_events_per_day >= p.p90_events_per_day
      or f.avg_sessions_per_day >= p.p90_sessions
    then true else false
  end as is_power_user
from f, p
