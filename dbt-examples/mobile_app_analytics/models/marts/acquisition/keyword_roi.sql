with revenue_by_user as (
  select
    u.user_id,
    u.signup_date,
    r.revenue
  from {{ ref('stg_users') }} u
  left join {{ ref('int_revenue_user_level') }} r
    on u.user_id = r.user_id
),
joined as (
  select
    a.keyword_id,
    a.keyword,
    a.campaign_id,
    ab.date,
    ab.cost,
    ab.conversions,
    coalesce(sum(rb.revenue), 0) as revenue
  from {{ ref('int_asa_keyword_performance') }} a
  left join {{ ref('int_asa_costs_by_day') }} ab
    on a.date = ab.date
    and a.campaign_id = ab.campaign_id
  left join revenue_by_user rb
    on rb.signup_date = a.date
  group by 1,2,3,4,5,6
)
select
  *,
  case when cost > 0 then revenue / cost else null end as roas
from joined
