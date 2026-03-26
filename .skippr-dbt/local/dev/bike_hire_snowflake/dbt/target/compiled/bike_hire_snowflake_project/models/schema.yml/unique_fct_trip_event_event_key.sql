
    
    

select
    event_key as unique_field,
    count(*) as n_records

from ANALYTICS.bike_hire_gold.fct_trip_event
where event_key is not null
group by event_key
having count(*) > 1


