with ug as (
  select * from {{ ref('user_growth') }}
),
eg as (
  select * from {{ ref('engagement') }}
),
rev as (
  select * from {{ ref('revenue_overview') }}
)
select
  ug.event_date as date,
  ug.new_users,
  ug.dau,
  eg.wau,
  eg.mau,
  eg.stickiness,
  rev.revenue,
  rev.ad_cost,
  rev.contribution_margin
from ug
left join eg on ug.event_date = eg.event_date
left join rev on ug.event_date = rev.date
