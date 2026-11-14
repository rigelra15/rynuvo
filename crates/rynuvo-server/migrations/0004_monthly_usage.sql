-- Relay bytes (in + out) per calendar month (UTC), for the data budget.
CREATE TABLE monthly_usage (
    period TEXT PRIMARY KEY,
    bytes INTEGER NOT NULL
);
