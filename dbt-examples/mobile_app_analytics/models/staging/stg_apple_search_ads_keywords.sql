select
  campaign_id,
  adgroup_id,
  keyword_id,
  cast(reporting_date as date) as date,
  keyword,
  match_type,
  bid_amount,
  impressions,
  taps,
  conversions,
  local_spend as cost
from {{ source('apple_ads', 'keywords') }}
