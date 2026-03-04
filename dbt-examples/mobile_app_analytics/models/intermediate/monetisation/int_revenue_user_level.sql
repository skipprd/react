select
  user_id,
  sum(revenue_usd) as revenue
from {{ ref('stg_iap_transactions') }}
group by 1
