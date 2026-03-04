{% macro cohortify(signup_date_col, event_date_col) %}
datediff('day', {{ signup_date_col }}, {{ event_date_col }})
{% endmacro %}
