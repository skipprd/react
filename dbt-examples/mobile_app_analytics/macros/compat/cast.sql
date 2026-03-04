{% macro as_string(expression) -%}
  {%- if target.type in ['athena', 'presto', 'trino'] -%}
    cast({{ expression }} as varchar)
  {%- else -%}
    cast({{ expression }} as string)
  {%- endif -%}
{%- endmacro %}





