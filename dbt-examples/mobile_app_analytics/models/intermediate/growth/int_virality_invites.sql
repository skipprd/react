select
  properties:sender_user_id::string as sender_user_id,
  properties:receiver_user_id::string as receiver_user_id,
  cast(event_ts as date) as date
from {{ ref('stg_events') }}
where event = 'Invite Sent'
