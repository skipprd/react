select
  event_id,
  user_id,
  anonymous_id,
  event,
  timestamp as event_ts,
  cast(timestamp as date) as event_date,
  properties,
  context,
  {{ json_text('context', 'page.path') }} as screen,
  {{ json_text('properties', 'boost_type') }} as boost_type,
  {{ json_text('properties', 'liked_user_id') }} as liked_user_id,
  {{ json_text('context', 'device.idfa') }} as device_ad_id
from {{ source('raw', 'events') }}
