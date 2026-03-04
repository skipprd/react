with cohort_revenue as (
  select
    u.signup_date as cohort_date,
    sum(r.revenue) as revenue
  from {{ ref('stg_users') }} u
  left join {{ ref('int_revenue_user_level') }} r
    on u.user_id = r.user_id
  group by 1
),
cohort_cost as (
  select
    date as cohort_date,
    sum(daily_cost) as cost
  from {{ ref('int_asa_costs_by_day') }}
  group by 1
)
select
  c.cohort_date,
  c.cost,
  r.revenue,
  case when c.cost > 0 then r.revenue / c.cost else null end as roas
from cohort_cost c
left join cohort_revenue r
  on c.cohort_date = r.cohort_date
