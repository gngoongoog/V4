# UI-09 Native Windows Responsive Layout and Reference Acceptance

Goal: compare actual Rust/Slint command center renders (not AI artwork) against the previously approved institutional design, with the chart and live operational widgets above the fold on typical monitor sizes.

## Fix
The UI-07/UI-08 dashboard had two KPI layout branches with `visible: false`. The hidden narrow layout still occupied a vertical box slot in the screenshot; this created a roughly 230px empty band between top KPI cards and live chart. UI-09 conditionally instantiates the correct 6-card single-row or 2x3 laptop grid so the hidden variant cannot reserve space. The main vertical dashboard gap is reduced to 9px.

## Native viewport matrix
The renderer still captures every one of the 22 pages under dark/white/smoke at 1120x820 and 880x700. The dashboard additionally captures real Slint pixels at 1024x768, 1280x800, 1366x768, 1540x920, 1600x900 and 1920x1080 in all three palettes. Screenshots are marked `RENDER FIXTURE / NOT LIVE MT5` and must never be misrepresented as broker data.

## Accessibility and realism
The interactive scroll containers remain available on small displays; the system does not promise that 22 complex panels will all fit above the fold at 880x700. The execution console and broker state are real Rust/Core bindings during runtime. The manual operator needs to test keyboard, screen scaling and actual MT5 Demo before running with money. Shadow/Demo/Live labels are links/actions, not a waiver of broker authorization, exposure or risk.
