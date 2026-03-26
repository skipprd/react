
    
    select
      count(*) as failures,
      count(*) != 0 as should_warn,
      count(*) != 0 as should_error
    from (
      
    
  
    
    

select
    rider_key as unique_field,
    count(*) as n_records

from ANALYTICS.bike_hire_gold.dim_rider
where rider_key is not null
group by rider_key
having count(*) > 1



  
  
      
    ) dbt_internal_test