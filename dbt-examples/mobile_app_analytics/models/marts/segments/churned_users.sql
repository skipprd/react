select
  user_id,
  inactivity_days,
  inactivity_days >= 14 as churned_14d,
  inactivity_days >= 30 as churned_30d
from {{ ref('int_churn_signals') }}
