# JSON Output Reference

The `--json` flag enables machine-readable JSON output for CI/CD pipelines, automation scripts, and programmatic consumption.

## Quick Start

```bash
timeout --json 30s ./my-command
```

Output is a single JSON object on stdout. The command's own stdout/stderr pass through normally.

## Schema Version

All JSON output includes a `schema_version` field. The current version is **3**.

```json
{"schema_version":3,"status":"completed",...}
```

Schema changes:

- **v1**: Initial release
- **v2**: Added `hook_*` fields for `--on-timeout` results
- **v3**: Added resource usage fields (`user_time_ms`, `system_time_ms`, `max_rss_kb`)

## Status Types

The `status` field indicates what happened:

| Status | Meaning |
|--------|---------|
| `completed` | Command finished before timeout |
| `timeout` | Command was killed due to timeout |
| `signal_forwarded` | timeout received a signal (SIGTERM/SIGINT/SIGHUP) and forwarded it to the child |
| `error` | timeout itself failed (command not found, permission denied, etc.) |

## Response Formats

### completed

Command finished normally before the timeout.

```json
{
  "schema_version": 3,
  "status": "completed",
  "exit_code": 0,
  "elapsed_ms": 1523,
  "user_time_ms": 45,
  "system_time_ms": 12,
  "max_rss_kb": 8432
}
```

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | integer | Schema version (currently 3) |
| `status` | string | Always `"completed"` |
| `exit_code` | integer | Command's exit code (0-255) |
| `elapsed_ms` | integer | Wall-clock time in milliseconds |
| `user_time_ms` | integer | User CPU time in milliseconds |
| `system_time_ms` | integer | System (kernel) CPU time in milliseconds |
| `max_rss_kb` | integer | Peak memory usage in kilobytes |

### timeout

Command was killed because it exceeded the time limit.

```json
{
  "schema_version": 3,
  "status": "timeout",
  "signal": "SIGTERM",
  "signal_num": 15,
  "killed": false,
  "command_exit_code": -1,
  "exit_code": 124,
  "elapsed_ms": 5003,
  "user_time_ms": 2100,
  "system_time_ms": 340,
  "max_rss_kb": 45000
}
```

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | integer | Schema version (currently 3) |
| `status` | string | Always `"timeout"` |
| `signal` | string | Signal sent to command (e.g., `"SIGTERM"`, `"SIGKILL"`) |
| `signal_num` | integer | Signal number (e.g., 15 for SIGTERM, 9 for SIGKILL) |
| `killed` | boolean | `true` if escalated to SIGKILL via `--kill-after` |
| `command_exit_code` | integer | Command's exit code, or -1 if killed by signal |
| `exit_code` | integer | timeout's exit code (124 by default, or custom via `--timeout-exit-code`) |
| `elapsed_ms` | integer | Wall-clock time in milliseconds |
| `user_time_ms` | integer | User CPU time in milliseconds |
| `system_time_ms` | integer | System (kernel) CPU time in milliseconds |
| `max_rss_kb` | integer | Peak memory usage in kilobytes |

#### With --on-timeout hook

When `--on-timeout` is specified, additional fields describe the hook execution:

```json
{
  "schema_version": 3,
  "status": "timeout",
  "signal": "SIGTERM",
  "signal_num": 15,
  "killed": false,
  "command_exit_code": -1,
  "exit_code": 124,
  "elapsed_ms": 5003,
  "user_time_ms": 2100,
  "system_time_ms": 340,
  "max_rss_kb": 45000,
  "hook_ran": true,
  "hook_exit_code": 0,
  "hook_timed_out": false,
  "hook_elapsed_ms": 150
}
```

| Field | Type | Description |
|-------|------|-------------|
| `hook_ran` | boolean | Whether the hook was executed |
| `hook_exit_code` | integer \| null | Hook's exit code, or `null` if timed out or failed to start |
| `hook_timed_out` | boolean | Whether the hook exceeded `--on-timeout-limit` |
| `hook_elapsed_ms` | integer | How long the hook ran in milliseconds |

### signal_forwarded

timeout received a signal (e.g., from `docker stop`, `kill`, or Ctrl+C) and forwarded it to the child process.

```json
{
  "schema_version": 3,
  "status": "signal_forwarded",
  "signal": "SIGTERM",
  "signal_num": 15,
  "command_exit_code": 143,
  "exit_code": 143,
  "elapsed_ms": 1200,
  "user_time_ms": 50,
  "system_time_ms": 10,
  "max_rss_kb": 4096
}
```

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | integer | Schema version (currently 3) |
| `status` | string | Always `"signal_forwarded"` |
| `signal` | string | Signal that was forwarded |
| `signal_num` | integer | Signal number |
| `command_exit_code` | integer | Command's exit code after receiving the signal |
| `exit_code` | integer | timeout's exit code (usually 128 + signal number) |
| `elapsed_ms` | integer | Wall-clock time in milliseconds |
| `user_time_ms` | integer | User CPU time in milliseconds |
| `system_time_ms` | integer | System (kernel) CPU time in milliseconds |
| `max_rss_kb` | integer | Peak memory usage in kilobytes |

### error

timeout itself encountered an error.

```json
{
  "schema_version": 3,
  "status": "error",
  "error": "command not found: nonexistent_cmd",
  "exit_code": 127,
  "elapsed_ms": 2
}
```

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | integer | Schema version (currently 3) |
| `status` | string | Always `"error"` |
| `error` | string | Human-readable error message |
| `exit_code` | integer | Exit code (125=internal error, 126=not executable, 127=not found) |
| `elapsed_ms` | integer | Wall-clock time in milliseconds |

Note: Error responses do **not** include resource usage fields since the command may not have started.

## Resource Usage Fields

Schema v3 added resource usage fields from the underlying `wait4()` syscall:

| Field | Description | Notes |
|-------|-------------|-------|
| `user_time_ms` | CPU time spent in user mode | Time the command spent executing application code |
| `system_time_ms` | CPU time spent in kernel mode | Time spent in system calls (I/O, memory allocation, etc.) |
| `max_rss_kb` | Peak resident set size | Maximum physical memory used, in kilobytes |

### Precision Notes

- Time values are **truncated** to milliseconds (not rounded) to avoid floating-point operations
- Memory values are truncated to whole kilobytes
- On macOS, `max_rss_kb` is derived from `ru_maxrss` (reported in bytes) divided by 1024

### Interpreting Resource Usage

**CPU-bound process:**

```json
{"user_time_ms": 4500, "system_time_ms": 100, "elapsed_ms": 4650}
```

High user time, low system time, elapsed ≈ user + system = CPU-bound.

**I/O-bound process:**

```json
{"user_time_ms": 50, "system_time_ms": 200, "elapsed_ms": 5000}
```

Low CPU times but high elapsed = waiting on I/O.

**Memory-intensive process:**

```json
{"max_rss_kb": 524288}
```

512 MB peak memory usage.

## Examples

### CI/CD Pipeline Integration

```bash
#!/bin/bash
result=$(timeout --json 5m ./run-tests 2>&1)
status=$(echo "$result" | jq -r '.status')

if [ "$status" = "completed" ]; then
    exit_code=$(echo "$result" | jq -r '.exit_code')
    echo "Tests completed with exit code $exit_code"
    exit $exit_code
elif [ "$status" = "timeout" ]; then
    elapsed=$(echo "$result" | jq -r '.elapsed_ms')
    echo "Tests timed out after ${elapsed}ms"
    exit 1
else
    error=$(echo "$result" | jq -r '.error')
    echo "Error: $error"
    exit 1
fi
```

### Resource Monitoring

```bash
#!/bin/bash
# Monitor build resource usage
result=$(timeout --json 30m make all 2>&1)

user_ms=$(echo "$result" | jq '.user_time_ms')
sys_ms=$(echo "$result" | jq '.system_time_ms')
mem_kb=$(echo "$result" | jq '.max_rss_kb')

echo "Build stats:"
echo "  CPU time: $((user_ms + sys_ms))ms (user: ${user_ms}ms, sys: ${sys_ms}ms)"
echo "  Peak memory: $((mem_kb / 1024))MB"
```

### Parsing with jq

```bash
# Get just the status
timeout --json 10s ./cmd | jq -r '.status'

# Check if timed out
timeout --json 10s ./cmd | jq '.status == "timeout"'

# Get resource usage as CSV
timeout --json 10s ./cmd | jq -r '[.elapsed_ms, .user_time_ms, .system_time_ms, .max_rss_kb] | @csv'
```

### Python Integration

```python
import subprocess
import json

result = subprocess.run(
    ["timeout", "--json", "30s", "./my-command"],
    capture_output=True,
    text=True
)

data = json.loads(result.stdout)

if data["status"] == "completed":
    print(f"Success! Used {data['max_rss_kb'] / 1024:.1f}MB memory")
elif data["status"] == "timeout":
    print(f"Timed out after {data['elapsed_ms']}ms")
    if data.get("killed"):
        print("Had to escalate to SIGKILL")
```

## Compatibility

- JSON output is always a single line (no pretty-printing)
- Field order may vary between versions; use a proper JSON parser
- New fields may be added in future schema versions
- Existing field semantics will not change within a major schema version

## See Also

- [README](../README.md) - General documentation
- `timeout --help` - Command-line help
