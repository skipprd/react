{% macro sessionize(table, user_col, timestamp_col, session_gap_minutes=30) %}
with base as (
  select
    {{ user_col }} as user_id,
    {{ timestamp_col }} as event_ts,
    lag({{ timestamp_col }}) over (partition by {{ user_col }} order by {{ timestamp_col }}) as prev_ts
  from {{ table }}
),
sessioned as (
  select
    user_id,
    event_ts,
    case
      when prev_ts is null then 1
      when {{ date_diff('minute', prev_ts, event_ts) }} > {{ session_gap_minutes }} then 1
      else 0
    end as new_session
  from base
)
select
  user_id,
  event_ts,
  sum(new_session) over (partition by user_id order by event_ts rows between unbounded preceding and current row) as session_id
from sessioned
{% endmacro %}
