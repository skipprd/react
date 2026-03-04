with base as (
  select event_date, user_id
  from {{ ref('int_user_daily_activity') }}
),
dau as (
  select event_date, count(distinct user_id) as dau
  from base
  group by 1
),
wau as (
  select
    event_date,
    sum(dau) over (
      order by event_date
      rows between 6 preceding and current row
    ) as wau
  from dau
),
mau as (
  select
    event_date,
    sum(dau) over (
      order by event_date
      rows between 29 preceding and current row
    ) as mau
  from dau
)
select
  d.event_date,
  d.dau,
  w.wau,
  m.mau,
  case when m.mau > 0 then d.dau::float / m.mau else null end as stickiness
from dau d
join wau w on d.event_date = w.event_date
join mau m on d.event_date = m.event_date
