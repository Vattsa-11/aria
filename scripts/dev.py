#!/usr/bin/env python3
import os, subprocess, sys, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WATCH = ["src", "assets", "Cargo.toml", "build.rs"]
DATA = os.environ.get("ARIA_DATA_DIR", os.path.join(os.path.expanduser("~"), ".aria-dev"))

def stamp():
    newest = 0.0
    for entry in WATCH:
        path = os.path.join(ROOT, entry)
        if os.path.isfile(path):
            newest = max(newest, os.path.getmtime(path))
            continue
        for folder, _, files in os.walk(path):
            for name in files:
                try:
                    newest = max(newest, os.path.getmtime(os.path.join(folder, name)))
                except OSError:
                    pass
    return newest

def build():
    print("\n== building ...", flush=True)
    started = time.time()
    result = subprocess.run(["cargo", "build"], cwd=ROOT)
    ok = result.returncode == 0
    print(f"== {'built' if ok else 'BUILD FAILED, keeping the old version running'} in {time.time() - started:.1f}s", flush=True)
    return ok

def launch():
    env = dict(os.environ, ARIA_DATA_DIR=DATA, ARIA_AGENT=os.environ.get("ARIA_AGENT", "1"))
    exe = os.path.join(ROOT, "target", "debug", "aria.exe" if os.name == "nt" else "aria")
    print(f"== starting {exe} (data: {DATA})", flush=True)
    return subprocess.Popen([exe] + sys.argv[1:], cwd=ROOT, env=env)

def stop(child):
    if child and child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()

def main():
    os.makedirs(DATA, exist_ok=True)
    print("Hot reload: edit any file under src/, assets/ or Cargo.toml and Aria rebuilds and restarts.\n"
          f"Live layout tweaks without a rebuild: edit {os.path.join(DATA, 'ui-live.json')}\nPress Ctrl+C to stop.")
    child = launch() if build() else None
    seen = stamp()
    try:
        while True:
            time.sleep(0.7)
            now = stamp()
            if now != seen:
                time.sleep(0.4)
                seen = stamp()
                if build():
                    stop(child)
                    child = launch()
            elif child is not None and child.poll() is not None:
                print(f"== Aria exited ({child.returncode}); waiting for a change to restart", flush=True)
                child = None
    except KeyboardInterrupt:
        pass
    finally:
        stop(child)

if __name__ == "__main__":
    main()
