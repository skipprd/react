select
  d.date,
  d.revenue,
  coalesce(a.daily_cost, 0) as ad_cost,
  d.revenue - coalesce(a.daily_cost, 0) as contribution_margin
from {{ ref('int_revenue_daily') }} d
left join {{ ref('int_asa_costs_by_day') }} a
  on d.date = a.date
