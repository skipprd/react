select
  date,
  campaign_id,
  adgroup_id,
  keyword_id,
  keyword,
  match_type,
  impressions,
  taps,
  cost,
  conversions,
  case when conversions > 0 then cost / conversions else null end as cpi
from {{ ref('stg_apple_search_ads_keywords') }}
