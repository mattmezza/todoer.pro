#!/usr/bin/env -S uv run --no-project --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["mcp==2.3.0"]
# ///
"""Test-only official MCP SDK OAuth interoperability against loopback fixtures.

The Rust test harness supplies an isolated synthetic browser session and CSRF.
This script does not sign in to Google, accept a bearer token, or support remote
production targets. All OAuth discovery, registration, PKCE, state validation,
and token exchange belong to the official SDK OAuthClientProvider.
"""

import asyncio
import ipaddress
import json
import os
import sys
import time
import uuid
from html.parser import HTMLParser
from urllib.parse import parse_qs, urlsplit

import httpx2
from mcp import ClientSession
from mcp.client.auth import OAuthClientProvider
from mcp.client.streamable_http import streamable_http_client
from mcp.shared.auth import AuthorizationCodeResult, OAuthClientMetadata

PHASE = "configuration"


class MemoryStorage:
    def __init__(self):
        self.tokens = None
        self.client = None

    async def get_tokens(self):
        return self.tokens

    async def set_tokens(self, tokens):
        self.tokens = tokens

    async def get_client_info(self):
        return self.client

    async def set_client_info(self, client_info):
        self.client = client_info


class HiddenFields(HTMLParser):
    def __init__(self):
        super().__init__()
        self.fields = {}

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "input" and attrs.get("type") == "hidden":
            self.fields[attrs.get("name")] = attrs.get("value", "")


def loopback_origin(url):
    parsed = urlsplit(url)
    assert (
        parsed.scheme == "http" and not parsed.username and not parsed.password
    )
    assert not parsed.fragment and parsed.hostname
    assert (
        parsed.hostname == "localhost"
        or ipaddress.ip_address(parsed.hostname).is_loopback
    )
    return f"{parsed.scheme}://{parsed.netloc}"


def tool_json(response):
    text = next(item.text for item in response.content if item.type == "text")
    return json.loads(text)


async def main():
    global PHASE
    url = os.environ.get("TODOER_MCP_URL", "http://127.0.0.1:8092/mcp")
    origin = loopback_origin(url)
    assert urlsplit(url).path == "/mcp" and not urlsplit(url).query
    cookie = os.environ["TODOER_TEST_SESSION_COOKIE"]
    csrf = os.environ["TODOER_TEST_CSRF"]
    callback_uri = "http://127.0.0.1:8765/sdk-oauth-callback"
    callback_result = None
    consent_calls = 0
    storage = MemoryStorage()

    async def redirect_handler(authorization_url):
        nonlocal callback_result, consent_calls
        global PHASE
        PHASE = "browser-consent"
        assert loopback_origin(authorization_url) == origin
        assert urlsplit(authorization_url).path == "/oauth/authorize"
        async with httpx2.AsyncClient(
            headers={"Cookie": cookie}, follow_redirects=False, timeout=30
        ) as browser:
            page = await browser.get(authorization_url)
            assert page.status_code == 200, "Consent page did not render"
            fields = HiddenFields()
            fields.feed(page.text)
            assert (
                fields.fields.get("csrf") == csrf
            ), "Consent CSRF did not match fixture"
            assert fields.fields.get(
                "request_id"
            ), "Consent request ID missing"
            approved = await browser.post(
                f"{origin}/oauth/approve",
                headers={"Origin": origin},
                data={
                    "request_id": fields.fields["request_id"],
                    "csrf": csrf,
                    "read": "yes",
                    "write": "yes",
                    "list_inbox": "yes",
                    "action": "allow",
                },
            )
            assert approved.status_code in (
                302,
                303,
            ), "Consent approval failed"
            location = approved.headers["location"]
            callback = urlsplit(location)
            assert (
                f"{callback.scheme}://{callback.netloc}{callback.path}"
                == callback_uri
            )
            query = parse_qs(callback.query)
            assert "error" not in query
            callback_result = AuthorizationCodeResult(
                code=query["code"][0],
                state=query.get("state", [None])[0],
                iss=query.get("iss", [None])[0],
            )
            consent_calls += 1

    async def callback_handler():
        assert callback_result is not None
        return callback_result

    provider = OAuthClientProvider(
        server_url=url,
        client_metadata=OAuthClientMetadata(
            client_name="Todoer isolated SDK OAuth check",
            redirect_uris=[callback_uri],
            token_endpoint_auth_method="none",
            grant_types=["authorization_code", "refresh_token"],
            response_types=["code"],
            scope="tasks:read tasks:write",
            application_type="native",
        ),
        storage=storage,
        redirect_handler=redirect_handler,
        callback_handler=callback_handler,
    )
    async with httpx2.AsyncClient(
        auth=provider, follow_redirects=False, timeout=30
    ) as client:
        async with streamable_http_client(url, http_client=client) as streams:
            async with ClientSession(*streams) as session:
                PHASE = "SDK-initialize-and-OAuth"
                await session.initialize()
                PHASE = "tool-list"
                tools = await session.list_tools()
                assert {
                    "resolve_destination",
                    "capture",
                    "read_subtree",
                    "search",
                } <= {tool.name for tool in tools.tools}

                async def call(name, arguments):
                    global PHASE
                    PHASE = f"tool-{name}"
                    response = await session.call_tool(name, arguments)
                    assert not response.is_error, f"Tool failed: {name}"
                    return tool_json(response)

                resolved = await call(
                    "resolve_destination", {"destination": "inbox"}
                )
                assert resolved["id"] == "inbox"
                payload = {
                    "list_id": "inbox",
                    "idempotency_key": f"oauth-sdk-{uuid.uuid4()}",
                    "tasks": [
                        {
                            "title": "Synthetic OAuth SDK check",
                            "description": "",
                            "parent_id": None,
                        }
                    ],
                }
                captured = await call("capture", payload)
                repeated = await call("capture", payload)
                assert len(captured) == 1 and repeated == captured
                subtree = await call("read_subtree", {"id": captured[0]["id"]})
                assert subtree[0]["id"] == captured[0]["id"]
                await call(
                    "configure_destination",
                    {"key": "oauth-sdk-inbox", "list_id": "inbox"},
                )
                assert (
                    await call(
                        "resolve_destination",
                        {"preference_key": "oauth-sdk-inbox"},
                    )
                )["id"] == "inbox"
                plan = await call(
                    "preview_changes",
                    {
                        "operations": [
                            {
                                "op": "update",
                                "id": captured[0]["id"],
                                "revision": captured[0]["revision"],
                                "title": "Reviewed OAuth SDK change",
                            }
                        ]
                    },
                )
                assert (await call("read_subtree", {"id": captured[0]["id"]}))[
                    0
                ]["title"] == captured[0]["title"]
                apply = {
                    "preview_id": plan["preview_id"],
                    "expected_hash": plan["expected_hash"],
                    "idempotency_key": f"oauth-sdk-apply-{uuid.uuid4()}",
                }
                applied = await call("apply_changes", apply)
                assert applied == await call("apply_changes", apply)
                assert (await call("read_subtree", {"id": captured[0]["id"]}))[
                    0
                ]["title"] == "Reviewed OAuth SDK change"
                PHASE = "scope-denial"
                denied = await session.call_tool(
                    "search", {"list": "groceries"}
                )
                assert (
                    denied.is_error
                ), "List outside consent unexpectedly accessible"
                assert tool_json(denied)["error"]["code"] == "forbidden"
                assert consent_calls == 1 and storage.tokens and storage.client
                assert storage.tokens.refresh_token, "Refresh token not issued"
                if os.environ.get("TODOER_TEST_REFRESH") == "1":
                    previous_refresh = storage.tokens.refresh_token
                    # Test-only expiry simulation; the SDK builds and performs
                    # the refresh grant and stores the replacement tokens.
                    PHASE = "SDK-refresh"
                    provider.context.token_expiry_time = time.time() - 60
                    await call("search", {"list": "inbox"})
                    assert storage.tokens.refresh_token != previous_refresh
                    assert (
                        consent_calls == 1
                    ), "Refresh unexpectedly required consent again"
                if os.environ.get("TODOER_TEST_REFRESH") == "1":
                    print(
                        "OAuth MCP SDK refresh passed: replacement refresh token stored; browser consent unchanged"
                    )
                print(
                    "OAuth MCP SDK check passed: discovery, registration, PKCE consent, token exchange, tools, capture retry, subtree, saved destination, bulk preview/apply/retry, list-scope denial"
                )
    return 0


if __name__ == "__main__":
    try:
        sys.exit(asyncio.run(main()))
    except Exception as error:
        # SDK and HTTP exceptions can carry credentials or authorization codes.
        print(
            f"OAuth MCP SDK check failed during {PHASE} ({type(error).__name__}); inspect the isolated test fixture",
            file=sys.stderr,
        )
        sys.exit(1)
