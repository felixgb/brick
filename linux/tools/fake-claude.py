#!/usr/bin/env python3
"""A stand-in for the Anthropic API, for trying the Add to To-do flow without a
real key or any network traffic. Always answers with the same two proposals,
dated relative to today.

    fake-claude.py 18765 &
    BRICK_ANTHROPIC_BASE_URL=http://127.0.0.1:18765 run-headless.sh restart
"""
import datetime as dt
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", 0)))
        d = dt.date.today()
        friday = d + dt.timedelta(days=(4 - d.weekday()) % 7 or 7)
        tuesday = d + dt.timedelta(days=(1 - d.weekday()) % 7 or 7)
        answer = {
            "items": [
                {"title": "Send the revised budget", "notes": "Version 3, with the venue deposit.", "location": "",
                 "date": friday.isoformat(), "end_date": None, "start_time": None, "end_time": None},
                {"title": "Planning call", "notes": "", "location": "Video call",
                 "date": tuesday.isoformat(), "end_date": None, "start_time": "10:00", "end_time": "11:00"},
                {"title": "Ana to book the venue", "notes": "For the 21st.", "location": "",
                 "date": None, "end_date": None, "start_time": None, "end_time": None},
            ],
            "summary": "Planning call notes: the budget is due Friday, and there is a call next Tuesday at 10:00.",
        }
        body = json.dumps({"stop_reason": "end_turn", "content": [{"type": "text", "text": json.dumps(answer)}]})
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body.encode())

    def log_message(self, *args):
        pass


HTTPServer(("127.0.0.1", int(sys.argv[1]) if len(sys.argv) > 1 else 18765), Handler).serve_forever()
