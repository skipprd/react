select
  transaction_id,
  user_id,
  purchase_date,
  product_id,
  revenue_usd
from {{ source('iap', 'transactions') }}
