with user_revenue as (
  select
    u.user_id,
    u.signup_date,
    coalesce(r.revenue, 0) as revenue
  from {{ ref('stg_users') }} u
  left join {{ ref('int_revenue_user_level') }} r
    on u.user_id = r.user_id
)
select
  signup_date as cohort_date,
  sum(revenue) as cohort_revenue,
  count(distinct user_id) as users,
  case when count(distinct user_id) > 0
    then sum(revenue) / count(distinct user_id)
    else null end as ltv
from user_revenue
group by 1
