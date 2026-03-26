
    
    select
      count(*) as failures,
      count(*) != 0 as should_warn,
      count(*) != 0 as should_error
    from (
      
    
  
    
    



select trip_key
from ANALYTICS.bike_hire_gold.fct_trip
where trip_key is null



  
  
      
    ) dbt_internal_test