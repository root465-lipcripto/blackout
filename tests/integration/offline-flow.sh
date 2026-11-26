#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
blackout_bin="$repo_root/target/debug/blackout"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
export XDG_CACHE_HOME="$scratch/cache"

source="$scratch/fixture"
prepared="$scratch/prepared"
restored="$scratch/restored"
bundle="$scratch/fixture.blackout"
mkdir -p "$source/src"
cat >"$source/Cargo.toml" <<'TOML'
[package]
name = "offline-fixture"
version = "0.1.0"
edition = "2021"

[dependencies]
itoa = "=1.0.15"
TOML
cat >"$source/src/lib.rs" <<'RS'
pub fn render(value: i32) -> String {
    itoa::Buffer::new().format(value).to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn renders_a_number() {
        assert_eq!(super::render(42), "42");
    }
}
RS
cargo generate-lockfile --manifest-path "$source/Cargo.toml"

"$blackout_bin" scan "$source" --json >"$scratch/scan.json"
grep -q 'DETECTED_CARGO' "$scratch/scan.json"
"$blackout_bin" prepare "$source" --output "$prepared"
"$blackout_bin" sandbox-check
"$blackout_bin" validate build "$prepared"
"$blackout_bin" validate test "$prepared"
"$blackout_bin" bundle create "$prepared" --output "$bundle"
"$blackout_bin" bundle verify "$bundle"
cache_result="$("$blackout_bin" cache put "$bundle")"
digest="$(printf '%s\n' "$cache_result" | sed -E 's/^sha256:([0-9a-f]{64}).*/\1/')"
"$blackout_bin" cache verify "$digest"
"$blackout_bin" restore "$bundle" "$restored"
"$blackout_bin" validate test "$restored"
"$blackout_bin" report "$restored" --json >"$scratch/report.json"
grep -q 'PARTIALLY_VERIFIED' "$scratch/report.json"
grep -q '"network_isolated": true' "$scratch/report.json"

cp "$bundle" "$scratch/corrupt.blackout"
truncate -s -1 "$scratch/corrupt.blackout"
if "$blackout_bin" bundle verify "$scratch/corrupt.blackout"; then
  echo "corrupt bundle was unexpectedly accepted" >&2
  exit 1
fi

echo "offline acceptance flow passed"
