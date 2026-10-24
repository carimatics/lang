#!/usr/bin/env python3
"""Independent RFC 0028 schema-5 packer; no compiler/runtime dependency."""
import struct
import sys

def u(n): return struct.pack("<I", n)
def q(n): return struct.pack("<Q", n)
def s(text):
    b = text.encode("utf-8")
    return u(len(b)) + b
def v(items): return u(len(items)) + b"".join(items)
def span(source, end): return u(source) + u(0) + u(end)
C = span(0, 5)
D = span(1, 2)
def pkg(): return s("demo") + s("artifact")
def interface(name): return pkg() + s(name)
def resource(): return interface("host") + s("counter")
def key(name): return interface("api") + s(name)
def version(major, minor, patch, pre=(), build=()):
    return q(major) + q(minor) + q(patch) + v([bytes([tag]) + s(text) for tag, text in pre]) + v([s(x) for x in build])
def i64_type(): return b"\x00" + C
def instruction(results, opcode, operands=b""):
    return v([u(r) for r in results]) + bytes([opcode]) + operands + C
def function(name, effect, params, result, capability):
    return s(name) + D + bytes([effect]) + D + v([s(n) + D + u(t) for n, t in params]) + u(result) + s(capability) + D + D

def reference():
    sources = v([s("reference.lang") + s("async"), s("reference.idl.json") + s("{}")])
    definitions = v([s("CounterKind") + b"\x01" + C + b"\x00" + v([])])
    constructed = v([b"\x12" + u(0) + C])  # ComponentResource = 18.
    imports = v([s("host_apply") + b"\x02" + C + v([i64_type()]) + i64_type()])
    functions = v([s("main") + b"\x01\x00\x01" + C + C + C + v([]) + i64_type() + v([])])
    places = v([v([i64_type()]) + v([])])
    ops = [
        instruction([0], 0, struct.pack("<q", 42)),
        instruction([], 39),
        instruction([], 41, u(0)),
        instruction([], 35, u(0)),
    ]
    block = v([]) + v(ops) + b"\x04" + C + C
    body = u(0) + u(0) + v([b"\x00" + C]) + v([]) + v([block])
    mir = sources + definitions + constructed + imports + functions + places + v([body]) + b"\x00"
    defs = [
        key("choice") + D + b"\x01" + v([s("case") + D + u(0)]) + D,
        key("number") + D + b"\x02" + u(1) + D,
        key("packet") + D + b"\x00" + v([]) + D,
    ]
    api = interface("api") + D + v([]) + v(defs) + v([
        function("main", 1, [], 1, "demo.artifact.main")
    ]) + D
    host = interface("host") + D + v([resource() + D]) + v([]) + v([
        function("apply", 0, [("n", 1)], 1, "demo.artifact.apply")
    ]) + D
    requirement = b"\x01" + version(1, 0, 0) + version(2, 0, 0) + b"\x01"
    imported = interface("host") + D + requirement + D + v([s("apply") + D]) + D
    types = [
        bytes([n]) + D for n in range(5)
    ] + [
        b"\x05" + u(1) + D, b"\x06" + u(3) + D,
        b"\x07" + u(1) + u(3) + D,
        b"\x08" + key("packet") + D + D,
        b"\x09" + resource() + D + D,
        b"\x0a\x00" + resource() + D + D,
        b"\x0a\x01" + resource() + D + D,
        b"\x05" + u(9) + D,
    ]
    document = u(1) + u(2) + pkg() + D + version(1, 2, 3, [(0, "1"), (1, "rc")], ["build", "001"]) + D
    document += v([api, host]) + v([imported]) + v([interface("api") + D]) + v(types)
    mappings = u(1) + v([u(0) + resource()]) + v([u(0) + interface("host") + s("apply")]) + v([u(0) + interface("api") + s("main")])
    payload = b"\x01" + mir + document + mappings
    return b"LANGBC\0\0" + struct.pack("<HHI", 5, 0, len(payload)) + payload

if __name__ == "__main__":
    data = reference()
    if "--binary" in sys.argv:
        sys.stdout.buffer.write(data)
    else:
        for i in range(0, len(data), 16):
            print(data[i:i + 16].hex(" "))
