
    
    select
      count(*) as failures,
      count(*) != 0 as should_warn,
      count(*) != 0 as should_error
    from (
      
    
  
    
    



select bike_key
from ANALYTICS.bike_hire_gold.dim_bike
where bike_key is null



  
  
      
    ) dbt_internal_test