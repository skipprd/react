with daily as (
  select
    d.event_date as date,
    d.user_id,
    coalesce(r.revenue, 0) as revenue
  from {{ ref('int_user_daily_activity') }} d
  left join {{ ref('int_revenue_user_level') }} r
    on d.user_id = r.user_id
)
select
  date,
  sum(revenue) as total_revenue,
  count(distinct user_id) as dau,
  case when count(distinct user_id) > 0
    then sum(revenue) / count(distinct user_id)
    else null end as arpu,
  case when count(distinct case when revenue > 0 then user_id end) > 0
    then sum(revenue) / count(distinct case when revenue > 0 then user_id end)
    else null end as arppu
from daily
group by 1
