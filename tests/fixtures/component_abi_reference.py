"""Independent RFC 0027 C reference vectors; reads Clang output, prints only."""
import hashlib
import json
import pathlib
import re
import struct
import subprocess
import sys

header = pathlib.Path(__file__).resolve().parents[2] / "docs/component-abi-v1.h"
text = re.sub(r"/\*.*?\*/", "", header.read_text(), flags=re.S)
prefix = "lang_component_v1_"
records = re.findall(r"typedef struct lang_component_v1_(\w+)\s*\{(.*?)\}\s*lang_component_v1_\w+;", text, re.S)

def compact(s):
    return " ".join(s.split())

def parameters(s):
    out = []
    for param in s.split(","):
        param = compact(param)
        # The reference has optional parameter names. Strip only the last
        # identifier, retaining unnamed scalar/struct/qualified pointer types.
        if "*" in param:
            param = re.sub(r"(?<=\*)\w+$", "", param)
        elif " " in param:
            param = param.rsplit(" ", 1)[0]
        out.append(param)
    return ",".join(out)

def H(*parts):
    parts = [p.encode() if isinstance(p, str) else p for p in parts]
    return hashlib.sha256(b"".join(struct.pack("<Q", len(p)) + p for p in parts)).digest()

callbacks = []
for result, name, args in re.findall(r"typedef\s+([\w\s*]+?)\s*\(LANG_COMPONENT_V1_CALL \*lang_component_v1_(\w+_fn)\)\((.*?)\);", text, re.S):
    result = compact(result)
    template = result + ("" if result.endswith("*") else " ") + "(LANG_COMPONENT_V1_CALL *@)(" + parameters(args) + ")"
    callbacks.append((name, template))
assert len(callbacks) == 5

def vectors(target):
    result = subprocess.run(["clang", "-target", target, "-x", "c", "-std=c11", "-Xclang", "-fdump-record-layouts-complete", "-fsyntax-only", str(header)], capture_output=True, text=True, check=True)
    actual = {}
    for block in result.stdout.split("*** Dumping AST Record Layout"):
        name = re.search(r"^\s*0 \| struct lang_component_v1_(\w+)\s*$", block, re.M)
        if not name:
            continue
        size, align = map(int, re.search(r"\[sizeof=(\d+), align=(\d+)\]", block).groups())
        offsets = [int(n) for n in re.findall(r"^\s*(\d+) \|   \S.*$", block, re.M)]
        actual[name[1]] = (size, align, offsets)
    digest = hashlib.sha256(H("LCAB1-layout", target))
    signatures = {name: H("LCAB1-signature", target, name, template).hex() for name, template in callbacks}
    layouts = {}
    for name, body in records:
        size, align, offsets = actual[name]
        fields = []
        for declaration in body.split(";"):
            declaration = compact(declaration)
            if not declaration:
                continue
            function = re.fullmatch(r"(\w+) \(LANG_COMPONENT_V1_CALL \*(\w+)\)\((.*)\)", declaration)
            if function:
                result, field, args = function.groups()
                template = parameters(args)
                signatures[field] = H("LCAB1-signature", target, field, result, "LANG_COMPONENT_V1_CALL", template).hex()
            else:
                base, field, array = re.fullmatch(r"(.*?[ *])(\w+)(\[\d+\])?", declaration).groups()
                template = base + "@" + (array or "")
            fields.append((field, template))
        assert len(fields) == len(offsets), name
        layouts[name] = {"size": size, "align": align, "offsets": {field: offset for (field, _), offset in zip(fields, offsets)}}
        digest.update(H(name, struct.pack("<Q", size), struct.pack("<Q", align)))
        for (field, template), offset in zip(fields, offsets):
            digest.update(H(field, template, struct.pack("<Q", offset)))
    query = "lang_component_v1_status LANG_COMPONENT_V1_CALL @(uint32_t,uint32_t,const lang_component_v1_api **,lang_component_v1_error *)"
    signatures["get_api"] = H("LCAB1-signature", target, "get_api", query).hex()
    assert len(signatures) == 38
    for signature in signatures.values():
        digest.update(bytes.fromhex(signature))
    return layouts, {"layout": digest.hexdigest(), "signatures": signatures}

targets = ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"]
layout, mac = vectors(targets[0])
other, windows = vectors(targets[1])
assert layout == other
pack = {"profile": "lang-component-c-v1", "status": "generated-reference-vectors", "targets": targets, "pointer_bytes": 8, "layout": layout, "fingerprints": dict(zip(targets, [mac, windows]))}
print(json.dumps(pack, indent=None if "--compact" in sys.argv else 2))
