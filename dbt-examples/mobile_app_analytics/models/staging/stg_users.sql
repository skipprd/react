select
  user_id,
  created_at,
  cast(created_at as date) as signup_date,
  country,
  device_platform
from {{ source('raw', 'users') }}
