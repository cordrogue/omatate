#!/usr/bin/env python3
"""Compare mutation response latency for two ui-notes binaries."""

import argparse
import json
import os
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def measure(binary: Path, entries_count: int, repetitions: int) -> list[float]:
    with tempfile.TemporaryDirectory(prefix="ui-notes-backend-bench-") as root:
        root = Path(root)
        session = root / "session"
        data = session / ".data"
        runtime = root / "runtime"
        data.mkdir(parents=True)
        runtime.mkdir()
        entries = [
            {
                "id": index,
                "ts": "2026-09-07T12:00:00",
                "ai": False,
                "status": "done",
                "transcript": "x" * 200,
            }
            for index in range(1, entries_count + 1)
        ]
        (data / "entries.jsonl").write_text(
            "".join(json.dumps(entry, separators=(",", ":")) + "\n" for entry in entries)
        )
        (data / "id").write_text("benchmark-token\n")
        (data / "draft.txt").write_text("")

        env = os.environ.copy()
        env.update(
            {
                "HOME": str(root),
                "XDG_CONFIG_HOME": str(root / "config"),
                "XDG_RUNTIME_DIR": str(runtime),
                "XDG_STATE_HOME": str(root / "state"),
                "UI_NOTES_SESSION_PATH": str(session),
            }
        )
        process = subprocess.Popen(
            [binary, "backend"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            env=env,
        )
        assert process.stdin is not None and process.stdout is not None
        timings = []
        try:
            for index in range(repetitions + 5):
                request = {
                    "id": index,
                    "cmd": "edit",
                    "session": str(session),
                    "token": "benchmark-token",
                    "entryId": entries_count,
                    "text": f"edited {index}",
                }
                start = time.perf_counter_ns()
                process.stdin.write(json.dumps(request, separators=(",", ":")) + "\n")
                process.stdin.flush()
                reply = json.loads(process.stdout.readline())
                elapsed = (time.perf_counter_ns() - start) / 1_000_000
                if not reply.get("ok"):
                    raise RuntimeError(reply)
                if index >= 5:
                    timings.append(elapsed)
        finally:
            process.stdin.close()
            process.wait(timeout=5)
        return timings


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--entries", type=int, default=10_000)
    parser.add_argument("--repetitions", type=int, default=25)
    args = parser.parse_args()

    for label, binary in (("baseline", args.baseline), ("candidate", args.candidate)):
        samples = measure(binary.resolve(), args.entries, args.repetitions)
        print(
            f"{label}: median={statistics.median(samples):.3f} ms "
            f"min={min(samples):.3f} ms max={max(samples):.3f} ms"
        )


if __name__ == "__main__":
    main()
