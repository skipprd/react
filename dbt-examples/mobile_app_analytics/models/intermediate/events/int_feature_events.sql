select
  event_date,
  user_id,
  event,
  screen,
  coalesce(boost_type, liked_user_id) as detail
from {{ ref('stg_events') }}
