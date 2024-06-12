create table if not exists images (
    url text primary key,
    generation int not null,
    last_used int not null,
    policy blob not null,
    data blob not null
)
