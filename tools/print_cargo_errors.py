#!/usr/bin/env python3
import sys

log_file = sys.argv[1] if len(sys.argv) > 1 else "/tmp/build_arm.log"
try:
    with open(log_file, "r", encoding="utf-8", errors="replace") as f:
        lines = f.readlines()
except Exception as e:
    print(f"::error::Could not read log file {log_file}: {e}")
    sys.exit(0)

found_error = False
for i, line in enumerate(lines):
    if "error[" in line or "error:" in line:
        found_error = True
        snippet = "".join(lines[max(0, i-1):min(len(lines), i+10)])
        # In GitHub Actions, %0A produces multiline annotations
        escaped = snippet.replace("\r", "").replace("\n", "%0A")
        print(f"::error::{escaped}")

if not found_error:
    tail = "".join(lines[-30:]).replace("\r", "").replace("\n", "%0A")
    print(f"::error::{tail}")
