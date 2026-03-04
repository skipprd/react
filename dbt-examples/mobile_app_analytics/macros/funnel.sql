{% macro funnel(steps, source_ref=ref('stg_events'), user_col='user_id', event_col='event', ts_col='event_ts') %}
select
  {{ user_col }} as user_id,
  {%- for step in steps %}
  min(case when {{ event_col }} = '{{ step['event'] }}' then {{ ts_col }} end) as {{ step['name'] }}{% if not loop.last %},{% endif %}
  {%- endfor %}
from {{ source_ref }}
group by 1
{% endmacro %}
