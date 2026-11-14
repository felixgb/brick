#!/usr/bin/env python3
"""Drive and inspect a running Brick (Linux) over its debug socket.

The app opens the socket when started with BRICK_DEBUG_SOCKET=<path>;
run-headless.sh does that. Examples:

    brickctl.py tree                      # outline of every visible widget
    brickctl.py shot /tmp/a.png           # screenshot of the main window
    brickctl.py shot /tmp/b.png itemEditor  # a particular window, by objectName
    brickctl.py set viewTabs index=2      # switch to the Calendar tab
    brickctl.py click 'text:Today'        # click a button by its text
    brickctl.py click monthGrid --pos 300,200 --double
    brickctl.py key Return                # press a key in the focused widget
    brickctl.py type 'Buy milk'
    brickctl.py action 'Next message'     # trigger a menu/toolbar action by its text
    brickctl.py call items.list           # a raw engine request
    brickctl.py script steps.txt          # many commands, one per line

Input commands (click, key, type, invoke) are queued by the app; follow them
with `wait 150` before reading the screen.
"""
import json
import os
import socket
import sys
import time

DEFAULT_SOCKET = os.environ.get(
    "BRICK_DEBUG_SOCKET",
    os.path.join(os.environ.get("BRICK_RUN_DIR", "/tmp/brick-debug"), "debug.sock"),
)


def send(sock_path, cmd, timeout=15.0):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(timeout)
    s.connect(sock_path)
    s.sendall(json.dumps(cmd).encode() + b"\n")
    buf = b""
    while not buf.endswith(b"\n"):
        chunk = s.recv(65536)
        if not chunk:
            break
        buf += chunk
    s.close()
    return json.loads(buf)


def outline(node, indent=0, out=None):
    """One line per widget: Class #name [x y w h] "text" and the interesting bits."""
    out = out if out is not None else []
    bits = [node["class"]]
    if "name" in node:
        bits.append("#" + node["name"])
    g = node.get("geometry")
    if g:
        bits.append("[%d %d %d %d]" % tuple(g))
    for key in ("title", "text", "value", "placeholder"):
        if node.get(key):
            bits.append('%s=%s' % (key, json.dumps(node[key], ensure_ascii=False)))
    if "checked" in node:
        bits.append("checked" if node["checked"] else "unchecked")
    if node.get("enabled") is False:
        bits.append("DISABLED")
    if node.get("focus"):
        bits.append("FOCUS")
    if "tabs" in node:
        bits.append("tabs=%s current=%d" % (node["tabs"], node["currentTab"]))
    if "rows" in node:
        bits.append("rows=%d" % node["rows"])
        if node.get("firstRows"):
            bits.append("first=" + json.dumps(node["firstRows"][:6], ensure_ascii=False))
    if "actions" in node:
        bits.append("actions=" + json.dumps([a["text"] for a in node["actions"] if a["visible"]], ensure_ascii=False))
    out.append("  " * indent + " ".join(bits))
    for child in node.get("children", []):
        outline(child, indent + 1, out)
    return out


def parse(argv):
    """Turn a command line into a protocol object."""
    cmd, rest = argv[0], argv[1:]
    if cmd in ("tree", "windows", "quit"):
        return {"cmd": cmd}
    if cmd == "shot":
        o = {"cmd": "screenshot", "path": os.path.abspath(rest[0])}
        if len(rest) > 1:
            o["window"] = rest[1]
        return o
    if cmd == "wait":
        return {"cmd": "wait", "ms": int(rest[0]) if rest else 100}
    if cmd == "click":
        o = {"cmd": "click", "target": rest[0]}
        if "--pos" in rest:
            x, y = rest[rest.index("--pos") + 1].split(",")
            o["pos"] = [int(x), int(y)]
        if "--double" in rest:
            o["double"] = True
        if "--right" in rest:
            o["button"] = "right"
        return o
    if cmd == "invoke":
        return {"cmd": "invoke", "target": rest[0], "method": rest[1]}
    if cmd == "action":
        return {"cmd": "action", "text": rest[0]}
    if cmd == "key":
        o = {"cmd": "key", "key": rest[0]}
        if len(rest) > 1 and not rest[1].startswith("--"):
            o["mod"] = rest[1]
        if "--target" in rest:
            o["target"] = rest[rest.index("--target") + 1]
        return o
    if cmd == "type":
        o = {"cmd": "type", "text": rest[0]}
        if "--target" in rest:
            o["target"] = rest[rest.index("--target") + 1]
        return o
    if cmd == "set":
        o = {"cmd": "set", "target": rest[0]}
        for kv in rest[1:]:
            k, v = kv.split("=", 1)
            o[k] = int(v) if k == "index" else (v == "true" if k == "checked" else v)
        return o
    if cmd == "call":
        return {"cmd": "call", "method": rest[0], "params": json.loads(rest[1]) if len(rest) > 1 else {}}
    if cmd == "raw":
        return json.loads(rest[0])
    raise SystemExit("unknown command: " + cmd)


def show(cmd, reply):
    if not reply.get("ok"):
        print("ERROR:", reply.get("error"), file=sys.stderr)
        return False
    if cmd["cmd"] == "tree":
        for w in reply["windows"]:
            print("\n".join(outline(w)))
    elif cmd["cmd"] == "windows":
        for w in reply["windows"]:
            flag = " (active)" if w["active"] else ""
            print("%s %r %s%s%s" % (w["class"], w["title"], w["geometry"], " modal" if w["modal"] else "", flag))
    elif cmd["cmd"] == "screenshot":
        print("%s (%dx%d) of %r" % (reply["path"], reply["width"], reply["height"], reply["window"]))
    elif cmd["cmd"] == "call":
        print(json.dumps(reply["value"], indent=2, ensure_ascii=False))
    return True


def main():
    argv = sys.argv[1:]
    sock = DEFAULT_SOCKET
    if argv[:1] == ["--socket"]:
        sock, argv = argv[1], argv[2:]
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    if argv[0] == "script":
        # One command per line, shell-style quoting; '#' starts a comment.
        import shlex

        status = 0
        for line in open(argv[1]):
            words = shlex.split(line, comments=True)
            if not words:
                continue
            cmd = parse(words)
            if not show(cmd, send(sock, cmd)):
                status = 1
            if cmd["cmd"] in ("click", "key", "type", "invoke", "action"):
                time.sleep(0.12)  # let the queued input land
        return status
    cmd = parse(argv)
    return 0 if show(cmd, send(sock, cmd)) else 1


if __name__ == "__main__":
    sys.exit(main())
