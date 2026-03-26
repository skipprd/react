
    
    select
      count(*) as failures,
      count(*) != 0 as should_warn,
      count(*) != 0 as should_error
    from (
      
    
  
    
    



select activity_date
from ANALYTICS.bike_hire_gold.agg_bike_daily_utilization
where activity_date is null



  
  
      
    ) dbt_internal_test