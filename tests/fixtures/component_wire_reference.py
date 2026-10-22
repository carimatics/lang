"""Independent RFC 0027 fixture packer; prints vectors and never writes files."""
import copy
import hashlib
import json
import pathlib
import struct

doc = json.loads(pathlib.Path(__file__).with_name("component_values.idl.json").read_text())
interface = doc["interfaces"][0]
key = interface["key"]
definitions = {t["name"]: t for t in interface["types"]}
function = next(f for f in interface["functions"] if f["name"] == "apply")

def u32(n):
    return struct.pack("<I", n)

def text(s):
    b = s.encode("utf-8")
    return u32(len(b)) + b

def value_key(name):
    return b"".join(text(s) for s in key.replace(":", "/").split("/")) + text(name)

def resolve(t):
    while isinstance(t, str) and t in definitions and "alias" in definitions[t]:
        t = definitions[t]["alias"]
    return t

reachable = set()

def schema(t):
    t = resolve(t)
    primitive = {"i64": 0, "bool": 1, "unit": 5, "text": 6, "bytes": 7}
    if isinstance(t, str):
        if t in primitive:
            return bytes([primitive[t]])
        if t not in reachable:
            reachable.add(t)
            d = definitions[t]
            for field in d.get("record", d.get("variant", [])):
                schema(field["type"])
        return bytes([11]) + value_key(t)
    kind, child = next(iter(t.items()))
    if kind == "result":
        return bytes([10]) + schema(child["ok"]) + schema(child["err"])
    return bytes([{"list": 8, "option": 9}[kind]]) + schema(child)

preimage = b"LCSG" + u32(1) + text(key + "." + function["name"])
preimage += bytes([0]) + text(function["capability"])
preimage += u32(len(function["params"]))
preimage += b"".join(schema(p["type"]) for p in function["params"])
preimage += schema(function["result"]) + u32(0) + u32(len(reachable))
for name in sorted(reachable):
    d = definitions[name]
    kind = "record" if "record" in d else "variant"
    preimage += value_key(name) + bytes([0 if kind == "record" else 1]) + u32(len(d[kind]))
    preimage += b"".join(text(f["name"]) + schema(f["type"]) for f in d[kind])
digest = hashlib.sha256(preimage).digest()

def pack(t, value):
    t = resolve(t)
    if isinstance(t, str):
        if t == "unit":
            return b""
        if t == "i64":
            return struct.pack("<q", value)
        if t == "bool":
            return bytes([value])
        if t == "text":
            return text(value)
        if t == "bytes":
            return u32(len(value)) + bytes(value)
        d = definitions[t]
        if "record" in d:
            return b"".join(pack(f["type"], value[f["name"]]) for f in d["record"])
        case = next(i for i, f in enumerate(d["variant"]) if f["name"] == value["case"])
        return u32(case) + pack(d["variant"][case]["type"], value["value"])
    kind, child = next(iter(t.items()))
    if kind == "list":
        return u32(len(value)) + b"".join(pack(child, v) for v in value)
    if kind == "option":
        return b"\0" if value is None else b"\1" + pack(child, value)
    arm, payload = next(iter(value.items()))
    return bytes([arm == "err"]) + pack(child[arm], payload)

def frame(kind, body):
    return (b"LCVL" + u32(1) + bytes([kind]) + digest + u32(len(body)) + body).hex()

leaf = {"code": 9, "flag": True, "label": "", "blob": []}
summary = {"empty": None, "total": -42, "enabled": True, "title": "あ😀", "raw": [0, 255, 1],
           "items": [{"case": "none", "value": None},
                     {"case": "entry", "value": {"code": -7, "flag": False, "label": "x", "blob": [2, 3]}}],
           "maybe": leaf, "outcome": {"err": "declared"}}
vectors = {"preimage": preimage.hex(), "digest": digest.hex(),
           "parameters": frame(0, pack("summary", summary)),
           "result_ok": frame(1, pack(function["result"], {"ok": summary})),
           "result_err": frame(1, pack(function["result"], {"err": "declared"}))}
alternate = copy.deepcopy(summary)
alternate.update(maybe=None, items=[], outcome={"ok": leaf})
vectors["parameters_none"] = frame(0, pack("summary", alternate))

# Separate report packer with field offsets for strict-tag mutation probes.
report = bytearray()
offsets = {}

def put(name, value):
    offsets[name] = len(report)
    report.extend(value)

def report_frame(name, mode, digest, member, span):
    put(name + "_mode", bytes([mode]))
    put(name + "_digest", digest)
    put(name + "_member", u32(member))
    put(name + "_span_present", bytes([span is not None]))
    if span is not None:
        for field, value in zip(["source", "start", "end"], span):
            put(name + "_span_" + field, u32(value))

def report_record(name, category, phase, diagnostic, message, limit, origin):
    put(name + "_category", bytes([category]))
    put(name + "_phase", bytes([phase]))
    put(name + "_diagnostic", text(diagnostic))
    put(name + "_message_present", bytes([message is not None]))
    if message is not None:
        put(name + "_message", text(message))
    put(name + "_limit_present", bytes([limit is not None]))
    if limit is not None:
        put(name + "_limit_kind", bytes([limit[0]]))
        put(name + "_limit_bound", struct.pack("<Q", limit[1]))
        put(name + "_limit_attempted", struct.pack("<Q", limit[2]))
    put(name + "_origin_present", bytes([origin is not None]))
    if origin is not None:
        report_frame(name + "_origin", *origin)

put("header", b"LCMP" + u32(3) + bytes([1]))
put("policy", bytes([1]))
report_record("primary", 5, 0, "component.limit", "bounded", (16, 128, 129),
              (2, bytes(range(32)), 42, (7, 3, 9)))
put("suppressed_count", u32(1))
report_record("suppressed", 2, 1, "component.boundary.destructor-failure", None, None,
              (3, bytes([3]) * 32, 13, None))
put("poisoned", bytes([1]))
put("omitted", struct.pack("<Q", 2**64 - 1))
for n in range(1, 10):
    put("usage_" + str(n), struct.pack("<Q", n))
put("frame_count", u32(5))
for mode in range(5):
    report_frame("frame_" + str(mode), mode, bytes([mode]) * 32, mode + 10,
                 (7, mode, mode + 1) if mode % 2 == 0 else None)
vectors["report_rich"] = report.hex()
vectors["report_offsets"] = offsets

def key_parts(k):
    return k.replace(":", "/").split("/")

def interface_key(k):
    return b"".join(text(p) for p in key_parts(k))

def contract_type(t, current):
    primitive = {"i64": 0, "bool": 1, "unit": 5, "text": 6, "bytes": 7}
    if isinstance(t, str):
        if t in primitive:
            return bytes([primitive[t]])
        owner, name = t.rsplit(".", 1) if "." in t else (current, t)
        return bytes([11]) + interface_key(owner) + text(name)
    kind, child = next(iter(t.items()))
    if kind in ["own", "borrow", "borrow-mut"]:
        owner, name = child.rsplit(".", 1) if "." in child else (current, child)
        return bytes([{"own": 2, "borrow": 3, "borrow-mut": 4}[kind]]) + interface_key(owner) + text(name)
    if kind == "result":
        return bytes([10]) + contract_type(child["ok"], current) + contract_type(child["err"], current)
    return bytes([{"list": 8, "option": 9}[kind]]) + contract_type(child, current)

def contract_fields(fields, current):
    return u32(len(fields)) + b"".join(text(f["name"]) + contract_type(f["type"], current) for f in fields)

def contract(locked):
    wire = bytearray()
    marks = {}
    ranges = {k: [] for k in ["resources", "types", "interfaces", "members", "requirements", "locks", "exports"]}
    def emit(name, b):
        marks[name] = len(wire)
        wire.extend(b)
    def version(v, label):
        base, *build = v.split("+", 1)
        core, *pre = base.split("-", 1)
        emit(label + "_core", struct.pack("<QQQ", *map(int, core.split("."))))
        parts = pre[0].split(".") if pre else []
        emit(label + "_pre_count", u32(len(parts)))
        for i, part in enumerate(parts):
            emit(label + f"_pre_{i}_tag", bytes([0 if part.isdigit() else 1]))
            emit(label + f"_pre_{i}_text", text(part))
        parts = build[0].split(".") if build else []
        emit(label + "_build_count", u32(len(parts)))
        for i, part in enumerate(parts):
            emit(label + f"_build_{i}", text(part))
    emit("header", b"LCMP" + u32(3) + bytes([0]))
    emit("package", interface_key(doc["package"]))
    version(doc["version"], "package_version")
    interfaces = sorted(doc["interfaces"], key=lambda i: key_parts(i["key"]))
    resources = sorted((key_parts(i["key"]) + [r], i["key"], r) for i in interfaces for r in i["resources"])
    emit("resources_count", u32(len(resources)))
    for _, owner, name in resources:
        start = len(wire)
        wire.extend(interface_key(owner) + text(name))
        ranges["resources"].append([start, len(wire)])
    types = sorted((key_parts(i["key"]) + [t["name"]], i["key"], t) for i in interfaces for t in i["types"])
    emit("types_count", u32(len(types)))
    for _, owner, t in types:
        start = len(wire)
        wire.extend(interface_key(owner) + text(t["name"]))
        kind = next(k for k in ["record", "variant", "alias"] if k in t)
        wire.extend(bytes([["record", "variant", "alias"].index(kind)]))
        wire.extend(contract_type(t[kind], owner) if kind == "alias" else contract_fields(t[kind], owner))
        ranges["types"].append([start, len(wire)])
    emit("interfaces_count", u32(len(interfaces)))
    for i in interfaces:
        start = len(wire)
        wire.extend(interface_key(i["key"]))
        functions = sorted(i["functions"], key=lambda f: f["name"])
        wire.extend(u32(len(functions)))
        for f in functions:
            member_start = len(wire)
            wire.extend(text(f["name"]) + bytes([f["effect"] == "task"]))
            wire.extend(contract_fields(f["params"], i["key"]) + contract_type(f["result"], i["key"]) + text(f["capability"]))
            if i["key"] == key:
                ranges["members"].append([member_start, len(wire)])
        ranges["interfaces"].append([start, len(wire)])
    imports = sorted(doc["imports"], key=lambda i: key_parts(i["interface"]))
    emit("requirements_count", u32(len(imports)))
    for index, requirement in enumerate(imports):
        start = len(wire)
        wire.extend(interface_key(requirement["interface"]))
        policy = requirement["version"]
        if "exact" in policy:
            wire.extend(bytes([0]))
            version(policy["exact"], f"requirement_{index}_exact")
        else:
            wire.extend(bytes([1]))
            version(policy["interval"]["min"], f"requirement_{index}_min")
            version(policy["interval"]["max"], f"requirement_{index}_max")
            emit(f"requirement_{index}_prerelease", bytes([policy["interval"]["prerelease"]]))
        members = sorted(requirement["members"])
        wire.extend(u32(len(members)) + b"".join(map(text, members)))
        ranges["requirements"].append([start, len(wire)])
    emit("locks_count", u32(2 if locked else 0))
    if locked:
        for index, (owner, v, digest) in enumerate([
            ("zzdemo:host/log", "1.4.0-beta.2+provider", bytes(range(32))),
            ("zzdemo:host/metrics", "3.2.1+audit.001", bytes([165]) * 32)
        ]):
            start = len(wire)
            wire.extend(interface_key(owner))
            version(v, f"lock_{index}_version")
            emit(f"lock_{index}_digest_tag", bytes([0]))
            wire.extend(digest)
            ranges["locks"].append([start, len(wire)])
    exports = sorted(doc["exports"], key=key_parts)
    emit("exports_count", u32(len(exports)))
    for owner in exports:
        start = len(wire)
        wire.extend(interface_key(owner))
        ranges["exports"].append([start, len(wire)])
    return wire.hex(), {"fields": marks, "ranges": ranges}

vectors["contract_unlocked"], _ = contract(False)
vectors["contract_locked"], vectors["contract_offsets"] = contract(True)

tick = b"LCSG" + u32(1) + text(key + ".tick") + bytes([1]) + text("demo.values.tick")
tick += u32(0) + bytes([0]) + u32(0) + u32(0)
tick_digest = hashlib.sha256(tick).digest()
vectors["tick_preimage"] = tick.hex()
vectors["tick_digest"] = tick_digest.hex()
vectors["tick_parameters"] = (b"LCVL" + u32(1) + bytes([0]) + tick_digest + u32(0)).hex()
vectors["tick_result"] = (b"LCVL" + u32(1) + bytes([1]) + tick_digest + u32(8) + struct.pack("<q", -(2**63))).hex()
counter_key = "demo:counter/api"
counter = b"LCSG" + u32(1) + text(counter_key + ".apply") + bytes([0]) + text("demo.counter.apply")
counter += u32(2) + contract_type({"borrow-mut": "counter"}, counter_key) + contract_type("request", counter_key)
counter += contract_type({"result": {"ok": "i64", "err": "text"}}, counter_key)
counter += u32(1) + interface_key(counter_key) + text("counter")
counter += u32(1) + interface_key(counter_key) + text("request") + bytes([0])
counter += contract_fields([{"name": "delta", "type": "i64"}, {"name": "label", "type": "text"}], counter_key)
vectors["counter_preimage"] = counter.hex()
vectors["counter_digest"] = hashlib.sha256(counter).hexdigest()
print(json.dumps(vectors, indent=2, sort_keys=True))
