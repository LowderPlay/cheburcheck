# Website API

The API uses PostgreSQL for operational data and ClickHouse for query, probe,
and feedback records. `CLICKHOUSE_URL` must identify an existing database over
native TCP, for example `clickhouse://user:password@host:9000/analytics`.
Percent-encode special characters in credentials. Add
`?stats_timezone=Asia%2FYekaterinburg` to change the complaint statistics timezone
from UTC.

## ClickHouse connection pool

All API requests share a Deadpool managed pool of independent native connections.
A small adapter creates and checks `klickhouse` connections; Deadpool handles pool
capacity, waiters, and connection reuse. Connections are opened on demand and
checked out exclusively for each operation.

| Environment variable | Default | Purpose |
| --- | --- | --- |
| `CLICKHOUSE_POOL_SIZE` | `4` | Maximum simultaneous connections per API process. |
| `CLICKHOUSE_ACQUIRE_TIMEOUT_SECONDS` | `5` | Deadline for waiting for a slot and establishing its connection. |
| `CLICKHOUSE_QUERY_TIMEOUT_SECONDS` | `10` | Deadline for each API read or write. |

Values must be positive integers. Docker Compose forwards these settings to the
website service. Connection setup also has a ten-second upper limit, bounded by
the acquisition deadline. Startup migrations have a separate 120-second deadline.

Cancelled, timed-out, and broken connections are closed and replaced on demand.
Reads retry once after a connection failure, releasing the failed connection
before acquiring another slot. Query timeouts are returned immediately. Writes
are never automatically retried because a failed response may follow a committed
insert.

## Tests

Run the regular website tests with the checked-in SQLx metadata:

```sh
SQLX_OFFLINE=true cargo test --locked -p website
```

The ClickHouse integration tests require a disposable native server. They create
and delete a temporary database, and the migration test needs database creation
permissions:

```sh
SQLX_OFFLINE=true CLICKHOUSE_TEST_URL=clickhouse://localhost:9000/default \
  cargo test --locked -p website analytics::tests -- --include-ignored
```
