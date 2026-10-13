"""Read back the app's live theme state over CDP (diagnostic)."""
import asyncio
import sys

sys.path.insert(0, "tools")
from cdp import WS, find_page  # noqa: E402


async def main(port: int):
    page = find_page(port)
    print("target url:", page.get("url"))
    ws = WS(page["webSocketDebuggerUrl"])
    await ws.connect()
    try:
        probes = {
            "storage theme   ": "localStorage.getItem('kilo-app-theme')",
            "storage ui mode ": "localStorage.getItem('kilo-app-ui-mode')",
            "data-theme      ": "document.documentElement.getAttribute('data-theme')",
            "data-ui-mode    ": "document.documentElement.getAttribute('data-ui-mode')",
            "prefers dark    ": "window.matchMedia('(prefers-color-scheme: dark)').matches",
            "nord rule found ": "[...document.styleSheets].some(s => { try { return [...s.cssRules].some(r => r.selectorText && r.selectorText.includes('nord')); } catch(e){ return false; } })",
            "ui-mode rule    ": "[...document.styleSheets].some(s => { try { return [...s.cssRules].some(r => r.selectorText && r.selectorText.includes('data-ui-mode')); } catch(e){ return false; } })",
            "sidebar bg token": "getComputedStyle(document.documentElement).getPropertyValue('--surface-sidebar').trim()",
            "blur token      ": "getComputedStyle(document.documentElement).getPropertyValue('--blur-panel').trim()",
            "bg-primary      ": "getComputedStyle(document.documentElement).getPropertyValue('--bg-primary').trim()",
            "body bg         ": "getComputedStyle(document.body).backgroundColor",
            "html attr theme ": "document.documentElement.getAttribute('data-theme')",
            "theme option knt": "localStorage.getItem('kilo-app-theme')",
            "all keys        ": "JSON.stringify(Object.fromEntries(Object.entries(localStorage)))".replace("Object.entries(localStorage)", "[...Array(localStorage.length)].map((_,i)=>[localStorage.key(i),localStorage.getItem(localStorage.key(i))])"),
        }
        for label, expr in probes.items():
            try:
                print(f"  {label}: {await ws.evaluate(expr)}")
            except Exception as exc:  # noqa: BLE001
                print(f"  {label}: ERROR {exc}")
    finally:
        await ws.close()


if __name__ == "__main__":
    asyncio.run(main(int(sys.argv[1])))
