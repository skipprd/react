
    
    

with child as (
    select rider_id as from_field
    from ANALYTICS.bike_hire_gold.fct_trip
    where rider_id is not null
),

parent as (
    select rider_id as to_field
    from ANALYTICS.bike_hire_gold.dim_rider
)

select
    from_field

from child
left join parent
    on child.from_field = parent.to_field

where parent.to_field is null


