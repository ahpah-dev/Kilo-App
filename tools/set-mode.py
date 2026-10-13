"""Switch the running app to a chat mode (agent/plan/chat) over CDP."""
import asyncio
import sys

sys.path.insert(0, "tools")
from cdp import WS, find_page  # noqa: E402

MODE_LABEL = {"agent": "Agent", "plan": "Plan", "chat": "Chat"}


async def main(port: int, mode: str):
    page = find_page(port)
    ws = WS(page["webSocketDebuggerUrl"])
    await ws.connect()
    try:
        label = MODE_LABEL[mode]
        result = await ws.evaluate(
            "(() => {"
            f"const btn = [...document.querySelectorAll('.mode-toggle button')].find(b => b.textContent.trim() === {label!r});"
            "if (!btn) return 'button not found';"
            "btn.click();"
            "return 'clicked ' + btn.textContent.trim(); })()"
        )
        print("mode switch:", result)
        await asyncio.sleep(1.0)
        state = await ws.evaluate(
            "(() => {"
            "const active = document.querySelector('.mode-toggle button.active');"
            "const notice = document.querySelector('.notice.plan');"
            "return (active ? active.textContent.trim() : 'none') + ' | plan notice: ' + (notice ? 'shown' : 'hidden'); })()"
        )
        print("state:", state)
    finally:
        await ws.close()


if __name__ == "__main__":
    asyncio.run(main(int(sys.argv[1]), sys.argv[2]))
