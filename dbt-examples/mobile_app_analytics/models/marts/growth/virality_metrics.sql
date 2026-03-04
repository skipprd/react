select
  date,
  count(*) as invites_sent,
  count(distinct receiver_user_id) as receivers,
  count(distinct sender_user_id) as senders
from {{ ref('int_virality_invites') }}
group by 1
