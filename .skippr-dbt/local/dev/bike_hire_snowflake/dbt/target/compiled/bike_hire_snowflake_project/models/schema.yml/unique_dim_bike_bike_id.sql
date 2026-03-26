
    
    

select
    bike_id as unique_field,
    count(*) as n_records

from ANALYTICS.bike_hire_gold.dim_bike
where bike_id is not null
group by bike_id
having count(*) > 1


