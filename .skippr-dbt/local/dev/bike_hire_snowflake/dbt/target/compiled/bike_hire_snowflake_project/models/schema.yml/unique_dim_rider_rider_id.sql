
    
    

select
    rider_id as unique_field,
    count(*) as n_records

from ANALYTICS.bike_hire_gold.dim_rider
where rider_id is not null
group by rider_id
having count(*) > 1


