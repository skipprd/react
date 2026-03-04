{% macro date_diff(unit, start_ts, end_ts) -%}
  {%- set unit_upper = unit|upper -%}
  {%- if target.type in ['snowflake', 'redshift'] -%}
    datediff({{ unit }}, {{ start_ts }}, {{ end_ts }})
  {%- elif target.type in ['athena', 'presto', 'trino'] -%}
    date_diff({{ unit }}, {{ start_ts }}, {{ end_ts }})
  {%- elif target.type in ['bigquery'] -%}
    {%- if unit_upper in ['SECOND','MINUTE','HOUR'] -%}
      timestamp_diff({{ end_ts }}, {{ start_ts }}, {{ unit_upper }})
    {%- else -%}
      date_diff(cast({{ end_ts }} as date), cast({{ start_ts }} as date), {{ unit_upper }})
    {%- endif -%}
  {%- else -%}
    -- Fallback assumes ANSI: try to cast to timestamp and compute epoch seconds; approximate for days
    {%- if unit_upper == 'SECOND' -%}
      extract(epoch from ({{ end_ts }} - {{ start_ts }}))
    {%- elif unit_upper == 'MINUTE' -%}
      extract(epoch from ({{ end_ts }} - {{ start_ts }})) / 60
    {%- elif unit_upper == 'HOUR' -%}
      extract(epoch from ({{ end_ts }} - {{ start_ts }})) / 3600
    {%- else -%}
      cast({{ end_ts }} as date) - cast({{ start_ts }} as date)
    {%- endif -%}
  {%- endif -%}
{%- endmacro %}

{% macro date_add(unit, interval, ts_expr) -%}
  {%- if target.type in ['snowflake', 'redshift'] -%}
    dateadd({{ unit }}, {{ interval }}, {{ ts_expr }})
  {%- elif target.type in ['athena', 'presto', 'trino'] -%}
    date_add({{ unit }}, {{ interval }}, {{ ts_expr }})
  {%- elif target.type == 'bigquery' -%}
    {%- set unit_upper = unit|upper -%}
    {%- if unit_upper in ['SECOND','MINUTE','HOUR'] -%}
      timestamp_add({{ ts_expr }}, interval {{ interval }} {{ unit_upper }})
    {%- else -%}
      date_add(cast({{ ts_expr }} as date), interval {{ interval }} {{ unit_upper }})
    {%- endif -%}
  {%- else -%}
    {{ ts_expr }} -- fallback no-op
  {%- endif -%}
{%- endmacro %}

{% macro day_of_week(date_expr) -%}
  {%- if target.type in ['snowflake', 'redshift'] -%}
    extract(dow from {{ date_expr }})
  {%- elif target.type in ['athena', 'presto', 'trino'] -%}
    day_of_week({{ date_expr }}) - 1
  {%- elif target.type == 'bigquery' -%}
    extract(DAYOFWEEK from {{ date_expr }}) - 1
  {%- else -%}
    extract(dow from {{ date_expr }})
  {%- endif -%}
{%- endmacro %}





