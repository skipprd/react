with daily as (
  select
    event_date,
    count(distinct case when event_date = signup_date then user_id end) as new_users,
    count(distinct user_id) as dau
  from {{ ref('int_retention_base') }}
  group by 1
)
select
  event_date,
  new_users,
  dau,
  sum(new_users) over (order by event_date) as cumulative_users
from daily
