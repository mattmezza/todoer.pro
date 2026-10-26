#!/usr/bin/env python3
"""Real local SMTP integration. Run: uv run --no-project --with aiosmtpd==1.4.6 python scripts/rebuild-smtp-check.py"""
import email
import email.policy
import os
import socket
import subprocess
from aiosmtpd.controller import Controller

class Capture:
    def __init__(self):
        self.messages = []
    async def handle_DATA(self, server, session, envelope):
        self.messages.append(envelope)
        return "250 captured"

capture = Capture()
with socket.socket() as probe:
    probe.bind(("127.0.0.1", 0))
    port = probe.getsockname()[1]
controller = Controller(capture, hostname="127.0.0.1", port=port)
controller.start()
try:
    env = os.environ.copy()
    for key in ("TODOER_SMTP_USERNAME", "TODOER_SMTP_PASSWORD", "TODOER_SMTP_TLS"):
        env.pop(key, None)
    env.update(TODOER_SMTP_HOST="127.0.0.1", TODOER_SMTP_PORT=str(port),
               TODOER_SMTP_FROM="sender@todoer.invalid", TODOER_SMTP_INSECURE_LOCAL="1",
               TODOER_SMTP_TEST_RECIPIENT="dev@todoer.invalid")
    subprocess.run(["cargo", "test", "-p", "todoer", "--lib",
                    "notifications::delivery_tests::smtp_capture_roundtrip", "--",
                    "--ignored", "--exact"], env=env, check=True)
    assert len(capture.messages) == 1, "exactly one queued message should be delivered"
    envelope = capture.messages[0]
    assert envelope.rcpt_tos == ["dev@todoer.invalid"]
    message = email.message_from_bytes(envelope.content, policy=email.policy.default)
    assert message["Message-ID"].endswith("@todoer.invalid>")
    assert message["Subject"] == "Todoer task reminder"
    assert "SMTP <test> & café" in message.get_content(), repr(message.get_content())
    subprocess.run(["cargo", "test", "-p", "todoer", "--lib",
                    "invitations::tests::smtp_invitation_delivery_retries_without_duplicate", "--",
                    "--ignored", "--exact"], env=env, check=True)
    assert len(capture.messages) == 2, "invitation worker must not replay sent mail"
    invite = email.message_from_bytes(capture.messages[1].content, policy=email.policy.default)
    assert "https://todoer.example.invalid/invitations/" in invite.get_content()
    assert "Private" not in invite.get_content(), "no private resource title in invite mail"
    print("SMTP capture passed: reminder + invitation recipients, stable Message-ID, Unicode body, private invitation link and no replay.")
finally:
    controller.stop()
