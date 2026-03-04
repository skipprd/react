select
  date,
  campaign_id,
  sum(impressions) as impressions,
  sum(taps) as taps,
  sum(cost) as cost,
  sum(conversions) as conversions,
  case when taps > 0 then taps::float / impressions else null end as ctr,
  case when taps > 0 then cost / taps else null end as cpt,
  case when conversions > 0 then cost / conversions else null end as cpi
from {{ ref('int_asa_keyword_performance') }}
group by 1,2
