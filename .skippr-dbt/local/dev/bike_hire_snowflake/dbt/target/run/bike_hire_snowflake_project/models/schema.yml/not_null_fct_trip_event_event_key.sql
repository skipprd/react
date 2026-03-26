
    
    select
      count(*) as failures,
      count(*) != 0 as should_warn,
      count(*) != 0 as should_error
    from (
      
    
  
    
    



select event_key
from ANALYTICS.bike_hire_gold.fct_trip_event
where event_key is null



  
  
      
    ) dbt_internal_test