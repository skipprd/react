select
  count(*) as total_users,
  sum(case when step_1_open is not null then 1 else 0 end) as step1,
  sum(case when step_2_onboarding is not null then 1 else 0 end) as step2,
  sum(case when step_3_signup is not null then 1 else 0 end) as step3
from {{ ref('int_funnels') }}
