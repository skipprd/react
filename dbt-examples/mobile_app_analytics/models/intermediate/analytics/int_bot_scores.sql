with session_stats as (
  select
    e.user_id,
    s.session_id,
    count(*) as events_per_session,
    datediff(
      'second',
      min(e.event_ts),
      max(e.event_ts)
    ) as session_duration_sec
  from {{ ref('stg_events') }} e
  join {{ ref('int_sessions') }} s
    on e.user_id = s.user_id
    and e.event_ts = s.event_ts
  group by 1,2
),
gaps as (
  select
    user_id,
    datediff(
      'second',
      lag(event_ts) over (partition by user_id order by event_ts),
      event_ts
    ) as inter_event_gap_sec
  from {{ ref('stg_events') }}
),
base as (
  select
    f.user_id,
    f.total_events,
    f.active_days,
    f.avg_events_per_day,
    f.avg_sessions_per_day,
    avg(s.events_per_session) as avg_events_per_session,
    avg(s.session_duration_sec) as avg_session_length,
    min(g.inter_event_gap_sec) as min_gap
  from {{ ref('int_behavioral_features') }} f
  left join session_stats s on f.user_id = s.user_id
  left join gaps g on f.user_id = g.user_id
  group by 1,2,3,4,5
)
select
  *,
  (
    case when total_events > 2000 then 1 else 0 end +
    case when avg_events_per_session > 150 then 1 else 0 end +
    case when avg_session_length < 3 then 1 else 0 end +
    case when min_gap is not null and min_gap < 1 then 1 else 0 end
  ) as bot_score
from base
