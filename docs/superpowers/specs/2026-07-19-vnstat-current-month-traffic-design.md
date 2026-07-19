# vnStat Current-Month Traffic Design

## Problem

When `stat_client` runs with `-n`/`--vnstat`, it reads `traffic.total` from `vnstat --json m`. That field is the interface's all-time total, so the reported `network_in` and `network_out` include every retained month instead of only the current month.

## Desired Behavior

- Keep invoking `/usr/bin/vnstat --json m`.
- Use the local calendar year and month as the reporting period.
- For every interface that passes the existing interface-name filter, inspect `traffic.month[]`.
- Add `rx` and `tx` only from entries whose `date.year` and `date.month` match the local current year and month.
- Sum matching current-month traffic across all included interfaces.
- Treat an interface with no current-month entry as contributing zero.
- Never fall back to the newest historical entry and never read `traffic.total` for monthly reporting.

## Implementation Structure

Extract a pure JSON parsing function that accepts the vnStat JSON text plus an explicit year and month. `get_vnstat_traffic` remains responsible for invoking vnStat and obtaining the local date, then delegates aggregation to the parser.

The parser will continue using `serde_json::Value`, matching the existing implementation and avoiding new dependencies or a large set of types for this focused change. Existing interface filtering remains unchanged.

## Error Handling

Missing current-month entries are valid and produce zero traffic. The existing behavior for malformed vnStat output or missing required top-level fields remains unchanged; broader command and JSON error handling is outside this fix.

## Testing

Add parser tests using inline vnStat JSON fixtures to verify:

- historical totals and previous-month entries are not counted;
- current-month entries from multiple included interfaces are summed;
- ignored interfaces do not contribute;
- data with no current-month entry returns `(0, 0)`.

Run the client tests and workspace tests after the fix.
