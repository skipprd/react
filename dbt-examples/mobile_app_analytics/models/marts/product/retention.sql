select
  signup_date,
  days_from_signup,
  count(distinct user_id) as retained_users
from {{ ref('int_retention_base') }}
group by 1,2
