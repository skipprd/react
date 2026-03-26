
    
    select
      count(*) as failures,
      count(*) != 0 as should_warn,
      count(*) != 0 as should_error
    from (
      
    
  
    
    

select
    bike_key as unique_field,
    count(*) as n_records

from ANALYTICS.bike_hire_gold.dim_bike
where bike_key is not null
group by bike_key
having count(*) > 1



  
  
      
    ) dbt_internal_test