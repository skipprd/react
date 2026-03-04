{% macro retention_matrix(base_ref=ref('int_retention_base'), day_buckets=[0,1,3,7,14,30]) %}
select
  signup_date,
  {%- for d in day_buckets %}
  sum(case when days_from_signup = {{ d }} then 1 else 0 end) as d{{ d }}{% if not loop.last %},{% endif %}
  {%- endfor %}
from {{ base_ref }}
group by 1
{% endmacro %}
