#!/usr/bin/env python3
import json, os, socket, sys

def data_dir():
    if os.environ.get("ARIA_DATA_DIR"):
        return os.environ["ARIA_DATA_DIR"]
    if os.name == "nt":
        return os.path.join(os.environ.get("APPDATA", "."), "Aria")
    return os.path.join(os.path.expanduser("~"), ".local", "share", "Aria")

def call(cmd, args=None, port=None, token=None):
    port = int(port or os.environ.get("ARIA_AGENT_PORT", 47474))
    if token is None:
        with open(os.path.join(data_dir(), "agent", "token")) as handle:
            token = handle.read().strip()
    with socket.create_connection(("127.0.0.1", port), timeout=30) as sock:
        request = {"id": 1, "cmd": cmd, "args": args or {}, "token": token}
        sock.sendall((json.dumps(request) + "\n").encode())
        data = b""
        while not data.endswith(b"\n"):
            chunk = sock.recv(65536)
            if not chunk:
                break
            data += chunk
    return json.loads(data)

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("usage: aria-agent.py <command> [json-args]\n  e.g. aria-agent.py state\n       aria-agent.py search '{\"query\": \"tum hi ho\"}'")
        sys.exit(2)
    arguments = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
    print(json.dumps(call(sys.argv[1], arguments), indent=2))
