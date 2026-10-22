#!/usr/bin/env python3
import sys
import os
import re
from datetime import datetime

def get_cargo_version():
    cargo_toml = os.path.join(os.path.dirname(__file__), "..", "Cargo.toml")
    if os.path.exists(cargo_toml):
        with open(cargo_toml, "r", encoding="utf-8") as f:
            for line in f:
                m = re.match(r'^\s*version\s*=\s*"([^"]+)"', line)
                if m:
                    return m.group(1)
    return None

def bump_changelog(version, date_str=None, changelog_path="CHANGELOG.md"):
    if date_str is None:
        date_str = datetime.now().strftime("%Y-%m-%d")

    if not os.path.exists(changelog_path):
        print(f"Error: {changelog_path} not found.")
        sys.exit(1)

    with open(changelog_path, "r", encoding="utf-8") as f:
        content = f.read()

    pattern = r"(##\s*\[CURRENT\]\s*\n)"
    match = re.search(pattern, content)
    if not match:
        print(f"Error: Could not find '## [CURRENT]' section in {changelog_path}.")
        sys.exit(1)

    idx = match.end()
    next_header = re.search(r"(##\s*\[[0-9]+\.[0-9]+)", content[idx:])

    if next_header:
        current_section = content[idx:idx + next_header.start()]
        rest = content[idx + next_header.start():]
    else:
        current_section = content[idx:]
        rest = ""

    new_current = (
        "## [CURRENT]\n\n"
        "### Breaking Changes\n\n"
        "### New Features\n\n"
        "### Fixes and Improvements\n\n"
    )

    released_section = f"## [{version}] - {date_str}\n" + current_section.rstrip() + "\n\n"

    new_content = content[:match.start()] + new_current + released_section + rest.lstrip()

    with open(changelog_path, "w", encoding="utf-8") as f:
        f.write(new_content)

    print(f"Successfully bumped CHANGELOG.md: [CURRENT] -> [{version}] - {date_str}")

def main():
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    changelog_path = os.path.join(repo_root, "CHANGELOG.md")

    if len(sys.argv) > 1:
        version = sys.argv[1]
    else:
        version = get_cargo_version()
        if not version:
            print("Error: Could not determine version from Cargo.toml and no version argument provided.")
            sys.exit(1)

    bump_changelog(version, changelog_path=changelog_path)

if __name__ == "__main__":
    main()
