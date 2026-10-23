"""Verify interactive offline preview in a real Chromium renderer (CI only)."""
from pathlib import Path
from playwright.sync_api import sync_playwright
import os

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'screenshots'
OUT.mkdir(exist_ok=True)
URL='http://127.0.0.1:8765/index.html?preview=1'
with sync_playwright() as playwright:
    browser=playwright.chromium.launch(headless=True,args=['--no-sandbox'])
    for width,height in ((1648,928),(1366,768)):
        page=browser.new_page(viewport={'width':width,'height':height},device_scale_factor=1)
        errors=[]
        page.on('pageerror',lambda error:errors.append(str(error)))
        page.goto(URL,wait_until='load')
        page.locator('#previewBanner').wait_for(state='visible')
        page.wait_for_timeout(650)
        assert page.locator('#candles').evaluate('(c)=>c.width>400 && c.height>100')
        assert page.locator('#startBtn').is_disabled(), 'Preview must NEVER send broker trades'
        assert page.locator('#equityValue').inner_text()!="—", 'Preview must show labelled sample data'
        assert len(page.locator('.kpi'))==6
        assert len(page.locator('.panel'))==9
        assert not errors,errors
        page.screenshot(path=str(OUT/f'dashboard-{width}x{height}-dark.png'),full_page=False)
        page.locator('[data-tf="m15"]').click()
        assert 'selected' in (page.locator('[data-tf="m15"]').get_attribute('class') or '')
        page.locator('[data-go="settings"]').first.click()
        page.locator('[data-theme="smoke"]').click()
        page.locator('[data-go="dashboard"]').first.click()
        page.screenshot(path=str(OUT/f'dashboard-{width}x{height}-smoke.png'),full_page=False)
        page.locator('[data-go="settings"]').first.click()
        page.locator('[data-theme="light"]').click()
        page.locator('[data-go="dashboard"]').first.click()
        page.screenshot(path=str(OUT/f'dashboard-{width}x{height}-light.png'),full_page=False)
        page.locator('#leavePreview').click()
        assert page.locator('#equityValue').inner_text()=="—", 'Real/offline mode must not retain fake equity'
        assert page.locator('#startBtn').is_disabled()
        assert not errors,errors
        page.close()
    browser.close()
print('Native-like dashboard viewport screenshots rendered and interactive controls checked.')