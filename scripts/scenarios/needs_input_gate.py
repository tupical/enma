#!/usr/bin/env python3
"""Real-binary scenario for the ADR-0007 §2 gate in enma_decide.

Drives enma-server over /v1/mcp in both operating modes: without an AI
provider and with a local chat-completions stub. Expected: a sensing item
that carries an unanswered question stops with needs_input, makes no model
call and persists nothing; the control item takes the normal path.

Usage: python3 -I needs_input_gate.py <enma-server binary> [expected git sha]
Prints one JSON line per step and exits 1 on the first mode with a mismatch.
"""
import base64
import hashlib
import hmac
import http.server
import json
import os
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

SECRET = "needs-input-gate-scenario"
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))
GATED = [
    ({"id": "sense_q", "kind": "question", "body": "Which DB?"}, ["Which DB?"]),
    ({"id": "sense_g", "kind": "research_gap", "body": "No load data"}, ["No load data"]),
    (
        {"id": "sense_o", "kind": "insight", "body": "hot keys",
         "open_questions": ["Who owns the cache?", " "]},
        ["Who owns the cache?"],
    ),
]
CONTROL = {"id": "sense_i", "kind": "insight", "body": "hot keys"}


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def b64(raw):
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode()


def token():
    claims = {"workspace": "scenario", "project": "scenario", "tool": "enma",
              "exp": int(time.time()) + 600}
    body = b64(json.dumps(claims).encode())
    return body + "." + b64(hmac.new(SECRET.encode(), body.encode(), hashlib.sha256).digest())


class Stub(http.server.BaseHTTPRequestHandler):
    calls = 0

    def do_POST(self):
        self.rfile.read(int(self.headers.get("content-length", 0)))
        Stub.calls += 1
        arguments = json.dumps({"statement": "Cache reads in Redis", "rationale": "Hot keys",
                                "reversibility": "irreversible"})
        reply = json.dumps({
            "id": "chatcmpl-stub", "object": "chat.completion", "created": 0, "model": "stub",
            "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
                "role": "assistant", "content": None,
                "tool_calls": [{"id": "call_1", "type": "function", "function": {
                    "name": "record_decision", "arguments": arguments}}]}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
        }).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(reply)))
        self.end_headers()
        self.wfile.write(reply)

    def log_message(self, *_):
        pass


def call(port, name, arguments):
    body = {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": name, "arguments": arguments}}
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}/v1/mcp", data=json.dumps(body).encode(),
        headers={"content-type": "application/json", "authorization": "Bearer " + token()})
    with OPENER.open(req, timeout=20) as resp:
        return json.load(resp)["result"]


def wait_health(port, server):
    for _ in range(200):
        if server.poll() is not None:
            raise SystemExit(f"enma-server exited early: {server.returncode}")
        try:
            with OPENER.open(f"http://127.0.0.1:{port}/healthz", timeout=1) as resp:
                return json.load(resp)
        except OSError:
            time.sleep(0.05)
    raise SystemExit("enma-server did not become healthy")


def run_mode(mode, binary, expected_sha, stub_port):
    failures = []

    def check(step, actual, expected):
        ok = actual == expected
        print(json.dumps({"mode": mode, "step": step, "ok": ok, "actual": actual,
                          "expected": expected}, ensure_ascii=False), flush=True)
        if not ok:
            failures.append(step)

    env = {k: v for k, v in os.environ.items()
           if not k.upper().endswith("_PROXY") and not k.startswith(("OPENAI_", "ENMA_"))}
    port = free_port()
    with tempfile.TemporaryDirectory(prefix="enma-gate-", dir=os.environ.get("SCENARIO_TMP")) as tmp:
        env.update(ENMA_PORT=str(port), ENMA_PLATFORM_SECRET=SECRET,
                   ENMA_DB=os.path.join(tmp, "enma.db"))
        if mode == "provider":
            env.update(OPENAI_API_KEY="x", OPENAI_BASE_URL=f"http://127.0.0.1:{stub_port}/v1",
                       OPENAI_API_PROTOCOL="chat_completions")
        with open(os.path.join(tmp, "server.log"), "wb") as log:
            server = subprocess.Popen([binary], env=env, stdout=log, stderr=log)
        try:
            health = wait_health(port, server)
            print(json.dumps({"mode": mode, "step": "healthz", "health": health}), flush=True)
            if expected_sha:
                check("git_sha", health.get("git_sha"), expected_sha)
            Stub.calls = 0
            for item, questions in GATED:
                result = call(port, "enma_decide", {"statement": "pick", "sensing_item": item})
                check(f"gate:{item['id']}",
                      (result.get("isError"), result.get("structuredContent")),
                      (True, {"error": "needs_input", "questions": questions}))
            hypothesis = {"id": "sense_h", "kind": "hypothesis", "body": "cache is slow"}
            result = call(port, "enma_decide", {"statement": "pick", "sensing_item": hypothesis})
            check("gate:hypothesis_keeps_adr_0011", result["structuredContent"].get("error"),
                  "not_decision_worthy")
            check("gate:ai_calls", Stub.calls, 0)
            check("gate:persisted", call(port, "enma_list", {})["structuredContent"]["decisions"], [])

            result = call(port, "enma_decide", {"statement": "hot keys dominate",
                                                "source_ref": "sense_i", "sensing_item": CONTROL})
            content = result["structuredContent"]
            if mode == "provider":
                check("control:is_error", result.get("isError", False), False)
                decision = content.get("decision", {})
                check("control:link", decision.get("links", [{}])[0].get("reference"), "sense_i")
                check("control:reversibility", decision.get("reversibility"), "irreversible")
                check("control:ai_calls", Stub.calls, 1)
                listed = call(port, "enma_list", {})["structuredContent"]["decisions"]
                check("control:persisted", len(listed), 1)
            else:
                check("control:error", content.get("error"), "ai_not_configured")
                check("control:ai_calls", Stub.calls, 0)
                listed = call(port, "enma_list", {})["structuredContent"]["decisions"]
                check("control:persisted", len(listed), 0)
        finally:
            server.terminate()
            server.wait(timeout=10)
        with open(os.path.join(tmp, "server.log"), "rb") as log:
            lowered = log.read().lower()
        check("server_log:panics", lowered.count(b"panicked"), 0)
    return failures


def main():
    binary = sys.argv[1]
    expected_sha = sys.argv[2] if len(sys.argv) > 2 else None
    stub = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Stub)
    threading.Thread(target=stub.serve_forever, daemon=True).start()
    failures = []
    for mode in ("no_provider", "provider"):
        failures += [f"{mode}/{step}" for step in
                     run_mode(mode, binary, expected_sha, stub.server_address[1])]
    stub.shutdown()
    print(json.dumps({"result": "fail" if failures else "pass", "failures": failures}), flush=True)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
