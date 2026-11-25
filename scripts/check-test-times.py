#!/usr/bin/env python3
"""Fail when a test a pull request runs took longer than the limit.

    check-test-times.py JUNIT [LIMIT]

JUNIT is the JUnit file of a run of the nextest `pr` profile (target/nextest/pr/junit.xml),
which skips the slow tests .config/nextest.toml lists; LIMIT is in seconds, 10 by default.
CI's check job runs it on main, where it sees each test at the runner's speed: a test past
the limit belongs in that list, or must get faster.
"""

import sys
import xml.etree.ElementTree as ET


def main() -> int:
    if len(sys.argv) not in (2, 3):
        print(__doc__.strip(), file=sys.stderr)
        return 2
    limit = float(sys.argv[2]) if len(sys.argv) == 3 else 10.0
    slow = sorted(
        (
            (float(case.get("time", "0")), case.get("classname", ""), case.get("name", ""))
            for case in ET.parse(sys.argv[1]).iter("testcase")
        ),
        reverse=True,
    )
    over = [row for row in slow if row[0] > limit]
    for seconds, binary, name in over:
        print(
            f"::error title=Test over {limit:g} s on a pull request::"
            f"{binary} {name} took {seconds:.1f} s"
        )
    if over:
        print(
            f"{len(over)} test(s) a pull request runs took over {limit:g} s. Add each to the"
            " `pr` profile's default-filter in .config/nextest.toml, or make it faster.",
            file=sys.stderr,
        )
        return 1
    top = slow[0] if slow else (0.0, "", "(none)")
    print(f"{len(slow)} tests, none over {limit:g} s; the slowest {top[1]} {top[2]} at {top[0]:.1f} s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
