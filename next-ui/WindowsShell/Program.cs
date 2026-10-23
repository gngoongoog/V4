using System.Diagnostics;
using System.Net;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using System.Security.Cryptography;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;

namespace BrainTradeNext;

internal static class Program
{
    [STAThread]
    private static void Main()
    {
        ApplicationConfiguration.Initialize();
        Application.Run(new TradingWindow());
    }
}

public sealed class TradingWindow : Form
{
    private readonly WebView2 _view = new() { Dock = DockStyle.Fill };
    private readonly HttpClient _http = new(new HttpClientHandler { UseProxy = false }) { Timeout = TimeSpan.FromSeconds(7) };
    private readonly SemaphoreSlim _startupLock = new(1, 1);
    private const string CoreHost = "http://127.0.0.1:18741";
    private const string TrustedOrigin = "https://braintrade.local/";
    private const string CoreStatus = "/v1/status";
    private bool _coreBootAttempted;

    public TradingWindow()
    {
        Text = "BRAINTRADE V4 — Institutional Trading System";
        ClientSize = new Size(1648, 928);
        MinimumSize = new Size(1080, 730);
        BackColor = Color.FromArgb(4, 15, 25);
        ForeColor = Color.White;
        StartPosition = FormStartPosition.CenterScreen;
        Controls.Add(_view);
        Shown += async (_, _) => await InitializeBrowserAsync();
        FormClosed += (_, _) => { _http.Dispose(); _startupLock.Dispose(); };
    }

    private async Task InitializeBrowserAsync()
    {
        try
        {
            string folder = Path.Combine(AppContext.BaseDirectory, "ui");
            string path = Path.Combine(folder, "index.html");
            if (!File.Exists(path)) throw new FileNotFoundException("UI files not installed", path);
            await _view.EnsureCoreWebView2Async();
            var browser = _view.CoreWebView2;
            browser.Settings.AreDevToolsEnabled = false;
            browser.Settings.AreDefaultContextMenusEnabled = false;
            browser.Settings.IsWebMessageEnabled = true;
            browser.Settings.IsStatusBarEnabled = false;
            browser.SetVirtualHostNameToFolderMapping("braintrade.local", folder, CoreWebView2HostResourceAccessKind.DenyCors);
            browser.NavigationStarting += (_, e) =>
            {
                // All JavaScript, CSS, charts and icons are local. Never navigate
                // away to untrusted sites or provide the backend key to JS.
                if (!e.Uri.StartsWith(TrustedOrigin, StringComparison.OrdinalIgnoreCase)) e.Cancel = true;
            };
            browser.NewWindowRequested += (_, e) => { e.Handled = true; };
            browser.WebMessageReceived += OnMessage;
            browser.Navigate(TrustedOrigin + "index.html");
        }
        catch (Exception ex)
        {
            MessageBox.Show($"تعذّر تشغيل واجهة BRAINTRADE NEXT.\n\n{ex.Message}\n\nتحتاج Windows WebView2 Runtime. تحقق من ملفات حزمة التطبيق.",
                "BrainTrade Next", MessageBoxButtons.OK, MessageBoxIcon.Error);
        }
    }

    private void OnMessage(object? sender, CoreWebView2WebMessageReceivedEventArgs e)
    {
        if (!e.Source.StartsWith(TrustedOrigin, StringComparison.OrdinalIgnoreCase)) return;
        try
        {
            using var d = JsonDocument.Parse(e.WebMessageAsJson);
            var root = d.RootElement;
            if (!root.TryGetProperty("id", out var idEl) || !idEl.TryGetInt32(out var id) || id <= 0) return;
            string verb = root.TryGetProperty("verb", out var v) ? v.GetString() ?? "" : "";
            string path = root.TryGetProperty("path", out var p) ? p.GetString() ?? "" : "";
            JsonElement? body = root.TryGetProperty("body", out var b) ? b.Clone() : null;
            _ = HandleMessageAsync(id, verb, path, body);
        }
        catch (JsonException) { /* An untrusted frame never gains Core access. */ }
    }

    private async Task HandleMessageAsync(int id, string verb, string path, JsonElement? body)
    {
        try
        {
            object result;
            if (verb == "APP" && path == "classic")
            {
                OpenClassic();
                result = new { opened = true };
            }
            else if (verb == "GET" && path == CoreStatus)
            {
                result = await SendAsync(HttpMethod.Get, path, null);
            }
            else if (verb == "POST" && (path == "/v1/start" || path == "/v1/stop"))
            {
                // Only the actual Rust Core can authorize mode/account/risk/execution.
                // Restrict mutation surface. Confirm REAL login a second time on server.
                if (path == "/v1/start" &&
                    (body is null || !body.Value.TryGetProperty("confirm_live_login", out _)))
                    throw new InvalidOperationException("Missing real-account confirmation field");
                result = await SendAsync(HttpMethod.Post, path, body);
            }
            else
                throw new InvalidOperationException("This Windows UI action is not permitted: " + verb + " " + path);
            Reply(id, true, result, null);
        }
        catch (Exception ex) { Reply(id, false, null, ex.Message); }
    }

    private void Reply(int id, bool ok, object? data, string? error)
    {
        // Response only to the trusted virtual-origin WebView. No JSON string
        // interpolation into JavaScript and no secrets exposed to client code.
        if (_view.IsDisposed || _view.CoreWebView2 is null) return;
        string payload = JsonSerializer.Serialize(new { id, ok, data, error });
        if (_view.InvokeRequired)
            _view.BeginInvoke(() => _view.CoreWebView2?.PostWebMessageAsJson(payload));
        else
            _view.CoreWebView2.PostWebMessageAsJson(payload);
    }

    private static string? FindIpcKeyPath()
    {
        // 'directories' (Rust) ProjectDirs may vary by Windows profile.
        string local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        string roaming = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        string[] candidates = {
            Path.Combine(local, "BrainTrade", "BrainTrade", "ipc.key"),
            Path.Combine(local, "BrainTrade", "ipc.key"),
            Path.Combine(roaming, "BrainTrade", "BrainTrade", "ipc.key"),
            Path.Combine(roaming, "BrainTrade", "ipc.key")
        };
        return candidates.FirstOrDefault(File.Exists);
    }

    private async Task<string> ReadCoreKeyAsync()
    {
        string? path = FindIpcKeyPath();
        if (path is null)
        {
            await TryBootCoreAsync();
            path = FindIpcKeyPath();
        }
        if (path is null) throw new InvalidOperationException("لم يتم العثور على مفتاح Core المحلي. شغّل برنامج V4 الأصلي مرة واحدة.");
        byte[] raw = await File.ReadAllBytesAsync(path);
        byte[] plain = ProtectedData.Unprotect(raw, null, DataProtectionScope.CurrentUser);
        return Encoding.UTF8.GetString(plain);
    }

    private async Task TryBootCoreAsync()
    {
        await _startupLock.WaitAsync();
        try
        {
            if (_coreBootAttempted) return;
            _coreBootAttempted = true;
            string exe = Path.Combine(AppContext.BaseDirectory, "BrainTrade.exe");
            if (!File.Exists(exe)) throw new FileNotFoundException("BrainTrade.exe (original Rust V4 engine) missing beside the Next app", exe);
            Process.Start(new ProcessStartInfo(exe, "--core")
            {
                UseShellExecute = false, CreateNoWindow = true,
                WorkingDirectory = AppContext.BaseDirectory,
                WindowStyle = ProcessWindowStyle.Hidden
            });
            // Core bootstraps its own gateway; do not start another or kill it when UI closes.
            for (int i = 0; i < 32; i++)
            {
                await Task.Delay(250);
                if (FindIpcKeyPath() is not null) break;
            }
        }
        finally { _startupLock.Release(); }
    }

    private async Task<JsonElement> SendAsync(HttpMethod method, string path, JsonElement? body)
    {
        string key = await ReadCoreKeyAsync();
        async Task<HttpResponseMessage> DispatchAsync()
        {
            using var request = new HttpRequestMessage(method, CoreHost + path);
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", key);
            if (body is not null)
                request.Content = new StringContent(body.Value.GetRawText(), Encoding.UTF8, "application/json");
            return await _http.SendAsync(request);
        }
        HttpResponseMessage res;
        try { res = await DispatchAsync(); }
        catch (HttpRequestException)
        {
            // Create a fresh request on retry; HttpRequestMessage cannot be sent twice.
            await TryBootCoreAsync();
            res = await DispatchAsync();
        }
        using (res)
        {
            string txt = await res.Content.ReadAsStringAsync();
            if (!res.IsSuccessStatusCode)
            {
                try
                {
                    using var problem = JsonDocument.Parse(txt);
                    if (problem.RootElement.TryGetProperty("error", out var message))
                        throw new InvalidOperationException(message.GetString() ?? "Core refused request");
                }
                catch (JsonException) { }
                throw new InvalidOperationException($"Rust Core returned {(int)res.StatusCode}: {res.ReasonPhrase}");
            }
            using var doc = JsonDocument.Parse(txt);
            return doc.RootElement.Clone();
        }
    }

    private void OpenClassic()
    {
        string exe = Path.Combine(AppContext.BaseDirectory, "BrainTrade.exe");
        if (!File.Exists(exe)) throw new FileNotFoundException("Original BrainTrade.exe not found", exe);
        Process.Start(new ProcessStartInfo(exe) { UseShellExecute = true, WorkingDirectory = AppContext.BaseDirectory });
    }
}