"""An ACP agent whose detached child holds a session lock after the agent stops."""

import fcntl
import json
import os
import pathlib
import subprocess
import sys
import time
import uuid

here = pathlib.Path.cwd()


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


if len(sys.argv) > 1 and sys.argv[1] == "--hold":
    with open(sys.argv[2], "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        (here / "locked").write_text(str(os.getpid()))
        deadline = time.monotonic() + 60
        while not (here / "release").exists() and time.monotonic() < deadline:
            time.sleep(0.05)
    sys.exit(0)

session_id = None

for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "initialize":
        send({"id": request["id"], "result": {"agentCapabilities": {"loadSession": True}}})
    elif method in ("session/new", "session/load"):
        if method == "session/load":
            session_id = request["params"]["sessionId"]
        elif session_id is None:
            session_id = f"held-{uuid.uuid4()}"
        with open("sessions.jsonl", "a") as log:
            log.write(json.dumps({"method": method, "pid": os.getpid(), "sessionId": session_id}) + "\n")
        if method == "session/load":
            with open(here / f"{request['params']['sessionId']}.lock", "a") as lock:
                while True:
                    try:
                        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                        break
                    except BlockingIOError:
                        time.sleep(0.05)
                send({"id": request["id"], "result": {"sessionId": session_id}})
        else:
            send({"id": request["id"], "result": {"sessionId": session_id}})
    elif method == "session/prompt":
        text = request["params"]["prompt"][0]["text"]
        if "HOLD" in text.split("\n"):
            subprocess.Popen(
                [sys.executable, __file__, "--hold", str(here / f"{session_id}.lock")],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            while not (here / "locked").exists():
                time.sleep(0.05)
            # No answer: the turn runs until the host stops it.
            continue
        send({"method": "session/update", "params": {"sessionId": session_id, "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "answered"},
        }}})
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})
    elif "id" in request:
        send({"id": request["id"], "result": {}})
