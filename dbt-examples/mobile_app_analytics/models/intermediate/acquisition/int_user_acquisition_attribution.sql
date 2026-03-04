with installs as (
  select
    user_id,
    signup_date,
    null as device_ad_id
  from {{ ref('stg_users') }}
),
asa as (
  select
    date,
    keyword,
    campaign_id,
    adgroup_id,
    keyword_id
  from {{ ref('int_asa_keyword_performance') }}
  where conversions > 0
)
select
  i.user_id,
  i.signup_date,
  a.campaign_id,
  a.adgroup_id,
  a.keyword_id,
  a.keyword
from installs i
left join asa a
  on i.signup_date = a.date
