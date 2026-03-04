with f as (
  select * from {{ ref('int_behavioral_features') }}
),
p as (
  select * from {{ ref('int_user_percentiles') }}
)
select
  f.user_id,
  f.avg_events_per_day,
  f.avg_sessions_per_day,
  case
    when f.avg_events_per_day <= p.p25_events_per_day
      and f.avg_sessions_per_day <= p.p25_sessions_per_day
    then true else false
  end as is_weak_user
from f, p
