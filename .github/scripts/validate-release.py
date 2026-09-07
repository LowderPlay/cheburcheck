"""Validate a component release tag against its Cargo manifest."""
import os
import sys
import tomllib
from pathlib import Path

component = sys.argv[1]
if component not in {"probe", "website", "reporter"}:
    raise SystemExit(f"Unknown release component: {component}")
with Path(component, "Cargo.toml").open("rb") as manifest:
    version = tomllib.load(manifest)["package"]["version"]
if os.environ.get("GITHUB_REF_TYPE") == "tag":
    expected = f"{component}-v{version}"
    actual = os.environ["GITHUB_REF_NAME"]
    if actual != expected:
        raise SystemExit(f"Tag {actual!r} does not match {component}/Cargo.toml; expected {expected!r}")
with open(os.environ["GITHUB_OUTPUT"], "a") as output:
    output.write(f"version={version}\n")
