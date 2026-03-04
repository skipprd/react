with screen_counts as (
  select
    screen,
    count(*) as seen
  from {{ ref('stg_events') }}
  where event = 'Screen Viewed'
  group by 1
)
select
  screen,
  seen
from screen_counts
order by seen desc
