create table prices
(
    id     bigserial PRIMARY KEY,
    symbol varchar(200) not null ,
    price  numeric(16,10) not null ,
    created_at timestamp not null
);

CREATE INDEX prices_timestamp_indexx ON prices (created_at DESC NULLS LAST);