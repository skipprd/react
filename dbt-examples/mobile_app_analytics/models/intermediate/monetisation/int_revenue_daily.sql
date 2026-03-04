select
  cast(purchase_date as date) as date,
  sum(revenue_usd) as revenue
from {{ ref('stg_iap_transactions') }}
group by 1
