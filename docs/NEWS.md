# Official news — 0.3.0

Built-in sources, no API key:

| Source | Feed |
| --- | --- |
| Federal Reserve monetary policy | https://www.federalreserve.gov/feeds/press_monetary.xml |
| ECB press/speeches | https://www.ecb.europa.eu/rss/press.html |
| ECB statistical releases | https://www.ecb.europa.eu/rss/statpress.html |

Source directories verified on 2026-10-12:
https://www.federalreserve.gov/feeds/feeds.htm and
https://www.ecb.europa.eu/home/html/rss.en.html.

The core starts a separate bounded RSS worker and refreshes every five minutes
after each batch. Requests time out after 12 seconds (connect timeout five).
Manual refresh is limited to once a minute. Disable/re-enable controls persist
across restart. Disabling discards the in-flight result and preserves saved data;
an already active request may finish before its thread exits.

SQLite stores each source's last attempt, last success, error and up to 60 items.
The dashboard shows up to 100 headlines sorted by valid publication time, with
cross-feed duplicate URLs removed. Feed failure never replaces a good cache
with an empty success. A source becomes stale after 15 minutes without success;
disabled/offline data is also marked cached. Unknown or future publication times
are explicit. Fresh retrieval is separate from article age: an old release can
be the latest item in a healthy source.

HTTPS certificates use the operating system's trusted roots; verification stays
enabled. Redirects are restricted to the original host. Only built-in official
HTTPS article hosts can be opened. RSS is limited to 1 MiB, depth 32, field 8 KiB;
DTD and custom entities are rejected. Titles render as plain text. The content
is data and cannot issue commands to the application.

Keyword tags are transparent labels, not sentiment, impact levels or price
predictions. These are published headlines, not a complete economic calendar.
They do not currently vote in Meta, block entries or enter a historical test;
there is no historical news archive or event-time replay yet.

APIs: GET /v1/news, POST /v1/news/refresh, POST /v1/news/settings {enabled:bool}.
All use the existing authenticated localhost IPC. BRAINTRADE_NEWS_OFFLINE=1 is
available for deterministic tests. Telegram /news returns cached source status
and headlines; /backtest returns the most recent saved historical summary.
No live Telegram delivery has been tested without the user's local token.
