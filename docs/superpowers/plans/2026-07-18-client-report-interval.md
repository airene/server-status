# Client Report Interval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a validated `-i`/`--interval` client option that configures the HTTP and gRPC report interval in seconds and defaults to 60 seconds.

**Architecture:** Keep interval parsing in the existing Clap `Args` structure and pass the parsed value through the existing `&Args` references. Both report loops convert the integer seconds to `Duration` at the sleep call; sampling behavior remains unchanged.

**Tech Stack:** Rust 2021, Clap 4.6 derive API, Tokio, Cargo tests

## Global Constraints

- The interval unit is integer seconds.
- The default interval is exactly 60 seconds.
- Both `-i 10` and `--interval 10` mean one report every 10 seconds.
- Values below 1 second and non-numeric values must be rejected during argument parsing.
- `-h` and `--help` must state that the interval unit is seconds.
- Compound values such as `10s` and `1m` are not supported.

---

### Task 1: Parse and apply the client report interval

**Files:**
- Modify: `client/src/main.rs:22-37,97`
- Modify: `client/src/grpc.rs:11-12,44`
- Modify: `README.md:27-31`
- Test: `client/src/main.rs`

**Interfaces:**
- Consumes: Clap's `Parser` and `CommandFactory` traits and the existing `Args` references passed to the report functions.
- Produces: `Args::interval: u64`, interpreted as seconds and guaranteed by Clap to be at least 1.

- [x] **Step 1: Write failing argument-parsing tests**

Append this test module to `client/src/main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::Args;
    use clap::{CommandFactory, Parser};

    #[test]
    fn report_interval_defaults_to_sixty_seconds() {
        let args = Args::try_parse_from(["stat_client"]).unwrap();
        assert_eq!(args.interval, 60);
    }

    #[test]
    fn short_report_interval_is_parsed_in_seconds() {
        let args = Args::try_parse_from(["stat_client", "-i", "10"]).unwrap();
        assert_eq!(args.interval, 10);
    }

    #[test]
    fn long_report_interval_is_parsed_in_seconds() {
        let args = Args::try_parse_from(["stat_client", "--interval", "10"]).unwrap();
        assert_eq!(args.interval, 10);
    }

    #[test]
    fn zero_report_interval_is_rejected() {
        assert!(Args::try_parse_from(["stat_client", "--interval", "0"]).is_err());
    }

    #[test]
    fn help_describes_report_interval_in_seconds() {
        let help = Args::command().render_help().to_string();
        assert!(help.contains("report interval in seconds"));
    }
}
```

- [x] **Step 2: Run the tests and verify RED**

Run:

```bash
cargo test -p stat_client
```

Expected: compilation fails because `Args` has no `interval` field. This is the intended RED failure proving that the tests require the new option.

- [x] **Step 3: Add the validated CLI option**

Remove `const INTERVAL_MS: u64 = 3000;` from `client/src/main.rs`. Add this field to `Args` after `pass`:

```rust
    #[clap(
        short = 'i',
        long,
        value_parser = clap::value_parser!(u64).range(1..),
        default_value_t = 60,
        help = "report interval in seconds"
    )]
    interval: u64,
```

- [x] **Step 4: Apply the option to HTTP and gRPC reporting**

In `client/src/main.rs`, replace the HTTP sleep call with:

```rust
        thread::sleep(Duration::from_secs(args.interval));
```

In `client/src/grpc.rs`, remove `use crate::INTERVAL_MS;` and replace the gRPC sleep call with:

```rust
        thread::sleep(Duration::from_secs(args.interval));
```

- [x] **Step 5: Update the feature status in the README**

Change the existing TODO entry to:

```markdown
- [x] 自定义数据上报间隔
```

- [x] **Step 6: Run focused tests and verify GREEN**

Run:

```bash
cargo test -p stat_client
```

Expected: all five new argument-parsing tests pass, along with any existing client tests.

- [x] **Step 7: Verify the real help output**

Run:

```bash
cargo run -p stat_client -- -h
```

Expected: exit code 0 and output containing an entry equivalent to `-i, --interval <INTERVAL>` with `report interval in seconds` and default value `60`.

- [x] **Step 8: Run repository-level checks**

Run:

```bash
cargo test --workspace
cargo fmt -p stat_client -- --check
git diff --check
```

Expected: every command exits successfully with no formatting or whitespace errors. The format check is scoped to `stat_client` because the pre-existing `server/src/stats.rs` does not pass the workspace-wide format check and is outside this feature's scope.

- [ ] **Step 9: Commit the implementation**

```bash
git add client/src/main.rs client/src/grpc.rs README.md docs/superpowers/plans/2026-07-18-client-report-interval.md
git commit -m "feat: configure client report interval"
```
