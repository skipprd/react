
    
    select
      count(*) as failures,
      count(*) != 0 as should_warn,
      count(*) != 0 as should_error
    from (
      
    
  
    
    



select trip_activity_date
from ANALYTICS.bike_hire_gold.fct_trip
where trip_activity_date is null



  
  
      
    ) dbt_internal_test