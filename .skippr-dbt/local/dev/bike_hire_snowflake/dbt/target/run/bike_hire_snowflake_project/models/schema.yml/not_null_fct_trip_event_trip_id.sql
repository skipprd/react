
    
    select
      count(*) as failures,
      count(*) != 0 as should_warn,
      count(*) != 0 as should_error
    from (
      
    
  
    
    



select trip_id
from ANALYTICS.bike_hire_gold.fct_trip_event
where trip_id is null



  
  
      
    ) dbt_internal_test