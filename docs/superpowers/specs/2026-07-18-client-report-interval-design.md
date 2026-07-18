# Client Report Interval Design

## Goal

Allow users to configure how often `stat_client` reports status data.

## Command-Line Interface

- Add `-i <SECONDS>` and `--interval <SECONDS>` to `stat_client`.
- Interpret the value as an integer number of seconds.
- Default to 60 seconds when the option is omitted.
- Reject zero, negative, and non-numeric values during command-line parsing.
- State the seconds unit in `-h` and `--help` output.
- Do not support compound duration values such as `10s` or `1m`.

Examples:

```text
stat_client -i 10
stat_client --interval 10
```

Both commands report once every 10 seconds.

## Runtime Behavior

Store the parsed interval on `Args` and use it in both HTTP and gRPC report loops. Convert the value with `Duration::from_secs` immediately before sleeping. Data sampling remains unchanged.

## Error Handling

Use a Clap value parser with a minimum value of 1. Invalid interval values cause the normal Clap usage error and prevent the client from starting.

## Testing

Add argument-parsing tests that verify:

- the default interval is 60 seconds;
- `-i 10` parses as 10 seconds;
- `--interval 10` parses as 10 seconds;
- zero is rejected;
- help output identifies the interval unit as seconds.

Run the client test suite and inspect the real `stat_client -h` output after implementation.
