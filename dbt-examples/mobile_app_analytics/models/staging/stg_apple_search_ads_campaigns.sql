select
  campaign_id,
  campaign_name,
  budget,
  budget_type,
  start_date,
  end_date
from {{ source('apple_ads', 'campaigns') }}
