select
  date,
  campaign_id,
  sum(cost) as daily_cost,
  sum(conversions) as conversions
from {{ ref('int_asa_keyword_performance') }}
group by 1,2
