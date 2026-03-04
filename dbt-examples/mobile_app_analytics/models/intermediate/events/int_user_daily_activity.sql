with joined as (
  select
    e.user_id,
    e.event_date,
    e.event,
    e.screen,
    s.session_id,
    e.event_ts
  from {{ ref('stg_events') }} e
  left join {{ ref('int_sessions') }} s
    on e.user_id = s.user_id
    and e.event_ts = s.event_ts
)
select
  user_id,
  event_date,
  count(*) as events,
  count(distinct session_id) as sessions,
  sum(case when event = 'Like' then 1 else 0 end) as likes,
  sum(case when event = 'Boost' then 1 else 0 end) as boosts,
  count(distinct screen) as screens_viewed,
  max(event_ts) as last_event_ts
from joined
group by 1,2
