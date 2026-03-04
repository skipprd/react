select
  user_id,
  bot_score,
  bot_score >= 2 as is_bot
from {{ ref('int_bot_scores') }}
