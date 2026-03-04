{% macro count_if(condition) -%}
  sum(case when {{ condition }} then 1 else 0 end)
{%- endmacro %}

{% macro sum_if(expression, condition) -%}
  sum(case when {{ condition }} then {{ expression }} else 0 end)
{%- endmacro %}





