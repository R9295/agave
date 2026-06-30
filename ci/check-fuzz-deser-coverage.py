#!/usr/bin/env python3
"""Verify that every type with a custom wincode schema implementation is
covered by the `custom_deser_roundtrip` fuzz harness.

A type is considered to have a *custom* schema implementation when it either:

  1. has a hand-written `impl ... SchemaRead/SchemaWrite ... for <Type>`, or
  2. derives the schema but customizes a field with `#[wincode(with = "...")]`.

Plain `#[derive(SchemaRead, SchemaWrite)]` types use the default, derived
codec and are intentionally out of scope -- only custom codecs need the extra
roundtrip coverage.

Such a type must appear in fuzz/fuzz_targets/custom_deser_roundtrip.rs, unless
it is listed in EXEMPT below (adapters, private deserialize helpers, generic
containers, nested sub-structs covered transitively, and test-only types).

When CI fails here, either:
  * add a new match arm for the type in custom_deser_roundtrip.rs, or
  * if the type is not a top-level wire payload (it's a helper/adapter/nested
    type covered through its parent), add it to EXEMPT with a one-line reason.
"""

import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HARNESS = os.path.join(
    REPO_ROOT, "fuzz", "fuzz_targets", "custom_deser_roundtrip.rs"
)
SKIP_DIRS = {"target", ".git", "fuzz"}

# Types with a custom schema impl that are intentionally NOT fuzzed as a
# top-level harness entry point. Keep each reason accurate -- this list is the
# audit trail for why a custom codec is exempt from direct roundtrip fuzzing.
EXEMPT = {
    # Generic serialization adapters / containers, exercised through the
    # concrete types that embed them.
    "BitVec": "generic container adapter, fuzzed via embedding types",
    "BitVecRef": "generic borrowed container adapter",
    "DefaultOnEmptyRead": "wincode-compat field adapter",
    "OptionCompat": "wincode-compat field adapter",
    "U32AsU64": "wincode-compat field adapter",
    "RejectNonzeroU8": "field adapter, fuzzed via CrdsData",
    "U16": "private varint wrapper in restart_crds_values",
    # Private deserialize-only helpers backing a parent's manual impl.
    "ContactInfoLite": "private deser helper for ContactInfo's manual impl",
    "PackedMinor": "private helper for Version's manual impl",
    "SerializedVersion": "private helper for Version's manual impl",
    # Nested sub-structs reached transitively from a fuzzed parent.
    "SlotMetaBase": "generic base of SlotMetaV3, covered via SlotMetaV3",
    "SlotMetaRepair": "alternate view of SlotMeta bytes, covered via SlotMetaV3",
    # Test-only.
    "WithFlags": "test-only struct in blockstore_meta tests",
}

IMPL_RE = re.compile(
    r"impl\b[^{]*\bSchema(?:Read|Write)\b[^{]*\bfor\s+([A-Za-z_][A-Za-z0-9_]*)"
)
WITH_RE = re.compile(r'#\[wincode\(with\s*=\s*"([^"]+)"\)\]')
TYPEDEF_RE = re.compile(r"\b(?:struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)")


def with_target_base(spec):
    """The leading type name in a `with = "..."` spec (the adapter type)."""
    spec = spec.strip()
    spec = re.split(r"[<\s]", spec, 1)[0]
    return spec.split("::")[-1]


def scan_repo():
    """Return (candidates, adapter_targets).

    candidates: {type_name: "rel/path.rs:line"} for types with a custom codec.
    adapter_targets: set of type names referenced inside `with = "..."`; these
    are serialization adapters, never payloads.
    """
    candidates = {}
    adapter_targets = set()

    for dirpath, dirnames, filenames in os.walk(REPO_ROOT):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for filename in filenames:
            if not filename.endswith(".rs"):
                continue
            path = os.path.join(dirpath, filename)
            rel = os.path.relpath(path, REPO_ROOT)
            try:
                lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
            except OSError:
                continue
            for i, line in enumerate(lines):
                impl_m = IMPL_RE.search(line)
                if impl_m:
                    candidates.setdefault(impl_m.group(1), f"{rel}:{i + 1}")
                with_m = WITH_RE.search(line)
                if with_m:
                    adapter_targets.add(with_target_base(with_m.group(1)))
                    # Attribute the custom field to its enclosing struct/enum.
                    for j in range(i, -1, -1):
                        type_m = TYPEDEF_RE.search(lines[j])
                        if type_m:
                            candidates.setdefault(type_m.group(1), f"{rel}:{j + 1}")
                            break
    return candidates, adapter_targets


def main():
    if not os.path.exists(HARNESS):
        print(f"error: harness not found at {HARNESS}", file=sys.stderr)
        return 1

    harness_src = open(HARNESS, encoding="utf-8").read()
    candidates, adapter_targets = scan_repo()

    missing = []
    for name, location in sorted(candidates.items()):
        if name in adapter_targets or name in EXEMPT:
            continue
        # Present if referenced as a whole word anywhere in the harness.
        if re.search(rf"\b{re.escape(name)}\b", harness_src):
            continue
        missing.append((name, location))

    # Surface stale exemptions so the list does not silently rot.
    live = set(candidates) | adapter_targets
    stale = sorted(name for name in EXEMPT if name not in live)
    if stale:
        print("warning: EXEMPT entries no longer found in source:")
        for name in stale:
            print(f"  - {name}")
        print()

    if missing:
        print("error: custom-schema types missing from the fuzz harness")
        print(f"  ({os.path.relpath(HARNESS, REPO_ROOT)}):\n")
        for name, location in missing:
            print(f"  - {name}  (defined at {location})")
        print()
        print("Each type with a hand-written SchemaRead/SchemaWrite impl or a")
        print('#[wincode(with = "...")] field must either get a match arm in the')
        print("harness, or be added to EXEMPT in ci/check-fuzz-deser-coverage.py")
        print("with a reason (if it's a helper/adapter/nested type, not a payload).")
        return 1

    covered = len(candidates) - len(adapter_targets & set(candidates)) - len(
        [n for n in EXEMPT if n in candidates]
    )
    print(
        f"ok: all custom-schema payload types are covered by the fuzz harness "
        f"({covered} checked, {len(EXEMPT)} exempt)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
