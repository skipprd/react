
    
    select
      count(*) as failures,
      count(*) != 0 as should_warn,
      count(*) != 0 as should_error
    from (
      
    
  
    
    



select rider_key
from ANALYTICS.bike_hire_gold.dim_rider
where rider_key is null



  
  
      
    ) dbt_internal_test