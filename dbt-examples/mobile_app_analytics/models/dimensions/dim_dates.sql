with spine as (
  {{ dbt_utils.date_spine(
      datepart="day",
      start_date="cast('2020-01-01' as date)",
      end_date="dateadd(day, 3650, cast('2020-01-01' as date))"
  ) }}
)
select
  date_day as date,
  extract(year from date_day) as year,
  extract(month from date_day) as month,
  extract(day from date_day) as day,
  {{ day_of_week('date_day') }} as day_of_week
from spine
