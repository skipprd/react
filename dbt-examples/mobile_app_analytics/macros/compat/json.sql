{% macro json_text(column_name, dotted_path) -%}
  {%- set json_path = '$.' ~ dotted_path -%}
  {%- if target.type in ['snowflake'] -%}
    {{ column_name }}{{ (':' ~ dotted_path|replace('.', ':')) }}::string
  {%- elif target.type in ['athena', 'presto', 'trino'] -%}
    json_extract_scalar({{ column_name }}, '{{ json_path }}')
  {%- elif target.type == 'bigquery' -%}
    json_value({{ column_name }}, '{{ json_path }}')
  {%- else -%}
    -- Fallback: try to parse as JSON and fetch text (may not work on all engines)
    json_extract_scalar({{ column_name }}, '{{ json_path }}')
  {%- endif -%}
{%- endmacro %}





