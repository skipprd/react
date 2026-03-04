select
  user_id,
  max(event_date) as last_active_date,
  datediff('day', max(event_date), current_date) as inactivity_days
from {{ ref('int_user_daily_activity') }}
group by 1
