create table prices
(
    id     bigserial PRIMARY KEY,
    symbol varchar(200) not null ,
    price  numeric(16,10) not null ,
    created_at timestamp not null
);
