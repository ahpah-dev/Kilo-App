"""Seed a conversation containing a task list and a plan, for UI verification."""
import asyncio
import json
import sys

sys.path.insert(0, "tools")
from cdp import WS, find_page  # noqa: E402


CONVERSATION = {
    "id": "verify-todos",
    "title": "Add a settings toggle",
    "model": "kilo-auto/free",
    "created_at": "2026-10-13T20:00:00.000Z",
    "updated_at": "2026-10-13T20:05:00.000Z",
    "todos": [
        {"id": "explore", "title": "Find where appearance settings live", "status": "done"},
        {"id": "types", "title": "Add the UiMode type and defaults", "status": "done"},
        {"id": "store", "title": "Persist the choice in the gateway store", "status": "in_progress"},
        {"id": "ui", "title": "Add the picker to Settings", "status": "pending"},
        {"id": "verify", "title": "Build and check it renders", "status": "pending"},
    ],
    "plan": {
        "summary": "Appearance is a theme plus a rendering mode, so the mode belongs next to the theme picker.",
        "changes": ["src/types/index.ts: add UiMode", "src/stores/gateway.ts: persist it"],
        "todos": [],
        "recordedAt": "2026-10-13T20:02:00.000Z",
    },
    "messages": [
        {
            "id": "m1",
            "role": "user",
            "content": "Add a UI mode picker and show me your task list.",
            "created_at": "2026-10-13T20:00:00.000Z",
        },
        {
            "id": "m2",
            "role": "assistant",
            "content": "Planned the work and recorded the task list on the left.",
            "created_at": "2026-10-13T20:01:00.000Z",
        },
    ],
}


async def main(port: int, theme: str, ui: str):
    page = find_page(port)
    ws = WS(page["webSocketDebuggerUrl"])
    await ws.connect()
    try:
        payload = json.dumps([CONVERSATION])
        expr = (
            "(() => {"
            f"localStorage.setItem('kilo-app-conversations', {json.dumps(payload)});"
            f"localStorage.setItem('kilo-app-theme', {json.dumps(theme)});"
            f"localStorage.setItem('kilo-app-ui-mode', {json.dumps(ui)});"
            "return localStorage.getItem('kilo-app-conversations').length; })()"
        )
        written = await ws.evaluate(expr)
        print(f"seeded conversations payload: {written} chars")
        await ws.call("Page.reload", {"ignoreCache": True})
    finally:
        await ws.close()

    await asyncio.sleep(3.5)
    page = find_page(port)
    ws = WS(page["webSocketDebuggerUrl"])
    await ws.connect()
    try:
        state = await ws.evaluate(
            "document.documentElement.getAttribute('data-ui-mode') + '/' +"
            " document.documentElement.getAttribute('data-theme')"
        )
        print("after reload:", state)

        # Open the seeded conversation so its task list is on screen.
        opened = await ws.evaluate(
            "(() => {"
            "const items = [...document.querySelectorAll('.conv-title')];"
            "const target = items.find(el => el.textContent.trim() === 'Add a settings toggle');"
            "if (!target) return 'conversation not found';"
            "target.closest('.conv').click();"
            "return 'clicked'; })()"
        )
        print("open conversation:", opened)
    finally:
        await ws.close()


if __name__ == "__main__":
    asyncio.run(main(int(sys.argv[1]), sys.argv[2], sys.argv[3]))
