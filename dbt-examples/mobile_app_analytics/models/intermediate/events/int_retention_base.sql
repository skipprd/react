select
  u.user_id,
  u.signup_date,
  d.event_date,
  datediff('day', u.signup_date, d.event_date) as days_from_signup
from {{ ref('stg_users') }} u
join {{ ref('int_user_daily_activity') }} d
  on u.user_id = d.user_id
