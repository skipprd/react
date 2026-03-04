select
  device_id,
  user_id,
  platform,
  os_version,
  app_version,
  first_seen_at,
  last_seen_at
from {{ source('raw', 'devices') }}
