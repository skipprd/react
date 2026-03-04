with daily as (
  select
    d.user_id,
    d.event_date,
    d.events,
    d.sessions,
    d.likes,
    d.boosts,
    d.screens_viewed,
    d.last_event_ts
  from {{ ref('int_user_daily_activity') }} d
)
select
  user_id,
  sum(events) as total_events,
  count(distinct event_date) as active_days,
  avg(events) as avg_events_per_day,
  sum(likes) as total_likes,
  sum(boosts) as total_boosts,
  avg(sessions) as avg_sessions_per_day,
  avg(screens_viewed) as avg_screens,
  max(last_event_ts) as last_active,
  datediff('day', max(last_event_ts), current_date) as days_since_last_seen
from daily
group by 1
