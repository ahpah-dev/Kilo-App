"""Exercise the 1.0.6 features in the running app over CDP.

Navigates the way a user would (the palette, then the sidebar) rather than
poking the history API, so this also covers the routing wiring.
"""
import asyncio
import json
import sys

sys.path.insert(0, "tools")
from cdp import WS, find_page  # noqa: E402


def conversation(title, messages):
    slug = title.replace(" ", "-").lower()
    return {
        "id": f"c-{slug}",
        "title": title,
        "model": "kilo-auto/free",
        "created_at": "2026-10-13T20:00:00.000Z",
        "updated_at": "2026-10-13T20:05:00.000Z",
        "messages": messages,
    }


SEED = [
    conversation(
        "Auth refactor",
        [
            {"id": "a1", "role": "user", "content": "Refactor the login flow to use the new token store.", "created_at": "2026-10-13T20:00:00.000Z"},
            {"id": "a2", "role": "assistant", "content": "Done. The session token now lives in the keyring.", "created_at": "2026-10-13T20:01:00.000Z"},
        ],
    ),
    conversation(
        "Rate limit handling",
        [
            {"id": "b1", "role": "user", "content": "Add automatic retry when the gateway rate limits us.", "created_at": "2026-10-13T20:02:00.000Z"},
            {"id": "b2", "role": "assistant", "content": "Added a 10s then 20s backoff for HTTP 429 rate limits.", "created_at": "2026-10-13T20:03:00.000Z"},
        ],
    ),
]


async def key(ws, k, code, vk, modifiers=0):
    for kind in ("keyDown", "keyUp"):
        await ws.call("Input.dispatchKeyEvent", {
            "type": kind, "modifiers": modifiers, "key": k, "code": code,
            "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk,
        })


async def type_into(ws, selector, value):
    """Set an input's value and fire the event Vue listens for."""
    return await ws.evaluate(
        "(() => {"
        f"const el = document.querySelector({json.dumps(selector)});"
        "if (!el) return false;"
        f"el.value = {json.dumps(value)};"
        "el.dispatchEvent(new Event('input', {bubbles: true}));"
        "return true; })()"
    )


async def open_palette(ws):
    await key(ws, "k", "KeyK", 75, modifiers=2)  # Ctrl+K
    await asyncio.sleep(0.7)


async def main(port: int, theme: str, ui: str):
    def say(step, value=None):
        print(f"[{step}] {'' if value is None else value}", flush=True)

    say("locating target")
    page = find_page(port)
    say("target", page.get("url"))
    ws = WS(page["webSocketDebuggerUrl"])
    await ws.connect()
    try:
        say("seeding storage")
        await ws.evaluate(
            "(() => {"
            f"localStorage.setItem('kilo-app-conversations', {json.dumps(json.dumps(SEED))});"
            f"localStorage.setItem('kilo-app-theme', {json.dumps(theme)});"
            f"localStorage.setItem('kilo-app-ui-mode', {json.dumps(ui)});"
            "return true; })()"
        )
        say("seeded; reloading")
        try:
            await ws.call("Page.reload", {"ignoreCache": True}, timeout=2)
        except TimeoutError:
            say("reload reply lost (expected)")
    finally:
        await ws.close()

    await asyncio.sleep(4)
    say("reconnecting")
    page = find_page(port)
    ws = WS(page["webSocketDebuggerUrl"])
    await ws.connect()
    say("connected")
    out = {}
    try:
        # --- 1. Palette opens on Ctrl+K --------------------------------------
        await open_palette(ws)
        out["1_palette_opens"] = await ws.evaluate("!!document.querySelector('.palette')")
        say("palette", out["1_palette_opens"])
        out["2_rows_rendered"] = await ws.evaluate("document.querySelectorAll('.palette .row').length")
        out["3_groups"] = await ws.evaluate(
            "[...document.querySelectorAll('.palette .group-label')].map(e => e.textContent.trim())"
        )

        # --- 2. Typing filters ------------------------------------------------
        await type_into(ws, ".palette input", "rate")
        await asyncio.sleep(0.4)
        out["4_filtered_labels"] = await ws.evaluate(
            "[...document.querySelectorAll('.palette .row .row-label')].map(e => e.textContent.trim())"
        )

        # --- 3. Enter on a chat result opens that conversation ----------------
        await type_into(ws, ".palette input", "auth refactor")
        await asyncio.sleep(0.4)
        await key(ws, "Enter", "Enter", 13)
        await asyncio.sleep(1.0)
        out["5_opened_title"] = await ws.evaluate(
            "document.querySelector('.titlebar .title')?.textContent.trim() ?? null"
        )
        out["6_palette_closed"] = await ws.evaluate("!document.querySelector('.palette')")

        # --- 4. Escape closes the palette -------------------------------------
        await open_palette(ws)
        await key(ws, "Escape", "Escape", 27)
        await asyncio.sleep(0.4)
        out["7_escape_closes"] = await ws.evaluate("!document.querySelector('.palette')")

        # --- 5. Palette can navigate to Search --------------------------------
        await open_palette(ws)
        await type_into(ws, ".palette input", "search all")
        await asyncio.sleep(0.4)
        await key(ws, "Enter", "Enter", 13)
        await asyncio.sleep(1.2)
        out["8_search_route"] = await ws.evaluate("!!document.querySelector('.search-view')")

        # --- 6. Full-text search ---------------------------------------------
        if out["8_search_route"]:
            await type_into(ws, ".search-bar input", "backoff")
            await asyncio.sleep(0.7)
            out["9_hits"] = await ws.evaluate("document.querySelectorAll('.search-view .hit').length")
            out["10_highlight"] = await ws.evaluate(
                "document.querySelector('.search-view .hit mark')?.textContent ?? null"
            )
            await type_into(ws, ".search-bar input", "zzznotpresent")
            await asyncio.sleep(0.7)
            out["11_no_match_state"] = await ws.evaluate(
                "!!document.querySelector('.search-view .hint-state')"
            )

        # --- 7. Sidebar: nav, palette hint, per-chat menu ---------------------
        out["12_nav_labels"] = await ws.evaluate(
            "[...document.querySelectorAll('.sidebar .nav-item .nav-label')].map(e => e.textContent.trim())"
        )
        out["13_palette_hint"] = await ws.evaluate("!!document.querySelector('.palette-hint')")
        out["14_conversations"] = await ws.evaluate("document.querySelectorAll('.sidebar .conv').length")

        await ws.evaluate(
            "(() => { const b = document.querySelector('.sidebar .conv .conv-menu-btn');"
            "if (b) b.click(); return !!b; })()"
        )
        await asyncio.sleep(0.5)
        out["15_menu_opens"] = await ws.evaluate("!!document.querySelector('.sidebar .conv-menu')")
        out["16_menu_items"] = await ws.evaluate(
            "[...document.querySelectorAll('.sidebar .conv-menu button')].map(e => e.textContent.trim())"
        )

        # --- 8. Pinning moves a chat into the Pinned group -------------------
        await ws.evaluate(
            "(() => { const b = [...document.querySelectorAll('.sidebar .conv-menu button')]"
            ".find(x => x.textContent.trim() === 'Pin'); if (b) b.click(); return !!b; })()"
        )
        await asyncio.sleep(0.6)
        out["17_groups_after_pin"] = await ws.evaluate(
            "[...document.querySelectorAll('.sidebar .group-label')].map(e => e.textContent.trim())"
        )
        out["18_pin_icon_visible"] = await ws.evaluate("!!document.querySelector('.sidebar .conv-pin')")

        print(json.dumps(out, indent=2))
    finally:
        await ws.close()


if __name__ == "__main__":
    asyncio.run(main(int(sys.argv[1]), sys.argv[2], sys.argv[3]))
