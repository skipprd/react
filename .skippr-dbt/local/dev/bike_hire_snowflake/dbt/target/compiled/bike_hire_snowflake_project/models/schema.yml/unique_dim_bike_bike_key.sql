
    
    

select
    bike_key as unique_field,
    count(*) as n_records

from ANALYTICS.bike_hire_gold.dim_bike
where bike_key is not null
group by bike_key
having count(*) > 1


