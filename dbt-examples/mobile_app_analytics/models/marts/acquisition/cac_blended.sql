with users_per_day as (
  select
    signup_date as date,
    count(distinct user_id) as new_users
  from {{ ref('stg_users') }}
  group by 1
)
select
  u.date,
  u.new_users,
  coalesce(a.daily_cost, 0) as paid_cost,
  case when u.new_users > 0 then coalesce(a.daily_cost, 0) / u.new_users else null end as cac
from users_per_day u
left join {{ ref('int_asa_costs_by_day') }} a
  on u.date = a.date
