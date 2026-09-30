"""Offline UI regression checks: python tests/ui-tooltips.py [--screenshot PATH]."""
import argparse
import json
from pathlib import Path
from playwright.sync_api import sync_playwright, expect


def account(name, five, weekly, **usage):
    return {
        "id": name, "email": f"{name}@example.test", "accountId": f"test-account-{name}", "plan": "plus",
        "usage": {
            "primary": {"usedPercent": 100 - five, "resetAfterSeconds": 2100},
            "secondary": None if weekly is None else {"usedPercent": 100 - weekly, "resetAfterSeconds": 86400},
            **usage,
        },
    }


parser = argparse.ArgumentParser()
parser.add_argument("--screenshot")
args = parser.parse_args()
accounts = [
    account("weekly-empty", 100, 0, resetCredits=10),
    account("low-weekly", 100, 13, resetCredits=3),
    account("balanced", 90, 52, resetCredits=2),
    account("best", 100, 73, resetCredits=4),
    account("expired", 100, 100, error="Status 401"),
    account("unknown", 100, None),
]
accounts[2]["isActive"] = True
fixture = {
    "success": True, "accounts": accounts, "activeAccount": accounts[2],
    "codexStatus": {"running": False}, "routerConfig": {"url": "", "hasPassword": False},
}
with sync_playwright() as p:
    browser = p.chromium.launch(channel="msedge", headless=True)
    page = browser.new_page(viewport={"width": 1080, "height": 940})
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.route("https://**/*", lambda route: route.abort())
    page.add_init_script("localStorage.setItem('codex_switcher_lang', 'tr'); window.__TAURI__ = {core: {invoke: async () => (" + json.dumps(fixture) + ")}};")
    page.goto((Path(__file__).resolve().parents[1] / "public" / "index.html").as_uri())
    expect(page.locator(".account-row")).to_have_count(6)
    assert page.locator(".row-email").all_text_contents() == [
        "best@example.test", "balanced@example.test", "low-weekly@example.test",
        "unknown@example.test", "weekly-empty@example.test", "expired@example.test",
    ]
    expect(page.locator(".best-choice-badge")).to_have_count(1)
    expect(page.locator(".account-row").first.locator(".best-choice-badge")).to_have_text("En iyi seçim")
    expect(page.locator("[title]")).to_have_count(0)
    unknown = page.locator(".account-row").filter(has_text="unknown@example.test")
    expect(unknown.locator(".mini-limit-val").nth(1)).to_have_text("—")

    tooltip = page.locator("#app-tooltip")
    quota = page.locator(".account-row").first.locator(".mini-limit-item").first
    quota.hover()
    expect(tooltip).to_be_visible()
    expect(tooltip).to_contain_text("5 saatlik kota")
    expect(tooltip).to_contain_text("Kalan: %100")
    expect(quota).to_have_attribute("aria-describedby", "app-tooltip")
    assert tooltip.evaluate("el => getComputedStyle(el).backgroundColor") == "rgb(24, 24, 27)"
    # Hovering the tooltip itself must keep its content visible.
    tooltip.hover()
    expect(tooltip).to_be_visible()
    page.keyboard.press("Escape")
    expect(tooltip).to_be_hidden()
    quota.focus()
    expect(tooltip).to_be_visible()
    page.keyboard.press("Escape")
    expect(tooltip).to_be_hidden()

    credits = page.locator(".mini-credits-pill").first
    credits.focus()
    expect(tooltip).to_contain_text("Sıfırlama Hakkı: 4")
    expect(tooltip).to_contain_text("Manuel")
    if args.screenshot:
        page.screenshot(path=args.screenshot)
    page.locator("#langBtnEn").click()
    expect(tooltip).to_be_hidden()
    page.locator(".row-error-badge").hover()
    expect(tooltip).to_contain_text("Session Expired")
    expect(tooltip).to_contain_text("Renew the session")
    page.locator("#openSettingsBtn").focus()
    expect(tooltip).to_contain_text("Settings")
    box = tooltip.bounding_box()
    assert box["x"] >= 12 and box["x"] + box["width"] <= 1080 - 12

    page.locator("#searchInput").fill("low-weekly")
    expect(tooltip).to_be_hidden()
    expect(page.locator(".account-row")).to_have_count(1)
    expect(page.locator(".best-choice-badge")).to_have_text("Best choice")
    page.locator("#searchInput").fill("")
    page.set_viewport_size({"width": 360, "height": 640})
    page.locator(".row-error-badge").focus()
    expect(tooltip).to_be_visible()
    box = tooltip.bounding_box()
    assert box["x"] >= 12 and box["x"] + box["width"] <= 348
    assert box["y"] >= 12 and box["y"] + box["height"] <= 628
    assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
    # A slow native status query must not accumulate more polling work.
    page.evaluate("""async () => {
        stopPolling();
        const original = window.__TAURI__.core.invoke;
        let calls = 0;
        let resolve;
        window.__TAURI__.core.invoke = () => {
            calls++;
            return new Promise(done => { resolve = done; });
        };
        const first = pollProcessStatus();
        await pollProcessStatus();
        if (calls !== 1) throw new Error('Overlapping status queries');
        resolve({success: false});
        await first;
        const next = pollProcessStatus();
        if (calls !== 2) throw new Error('Polling did not resume');
        resolve({success: false});
        await next;
        window.__TAURI__.core.invoke = async () => { throw new Error('test failure'); };
        await pollProcessStatus();
        if (processPollInFlight) throw new Error('Failed poll remained locked');
        window.__TAURI__.core.invoke = original;
    }""")
    assert not errors, errors
    browser.close()
print("UI checks passed: ranking, tooltip hover/focus/Escape, localization, filtering, unknown quota, and 360px viewport.")
