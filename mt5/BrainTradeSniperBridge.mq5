//+------------------------------------------------------------------+
//| BrainTradeSniperBridge.mq5                                      |
//| Native OnTick transport only. It never submits broker orders.   |
//+------------------------------------------------------------------+
#property strict
#property version   "1.00"
#property description "BrainTrade native OnTick bridge. Tick transport only; no trading calls."

input string InpPipeName = "\\\\.\\pipe\\BrainTradeSniperV1";
input int    InpReconnectMs = 250;
input bool   InpShowStatus = true;

#define TOKEN_FILE "BrainTrade\\sniper.token"

int    g_pipe = INVALID_HANDLE;
string g_token = "";
ulong  g_next_retry_ms = 0;
ulong  g_sent_ticks = 0;
ulong  g_failed_ticks = 0;

void ClosePipe()
  {
   if(g_pipe != INVALID_HANDLE)
     {
      FileClose(g_pipe);
      g_pipe = INVALID_HANDLE;
     }
  }

bool LoadSessionToken()
  {
   ResetLastError();
   int token_file = FileOpen(TOKEN_FILE, FILE_READ|FILE_TXT|FILE_ANSI);
   if(token_file == INVALID_HANDLE)
      return false;

   string token = FileReadString(token_file);
   FileClose(token_file);
   StringReplace(token, "\r", "");
   StringReplace(token, "\n", "");
   if(StringLen(token) < 16)
      return false;
   g_token = token;
   return true;
  }

bool EnsurePipe()
  {
   if(g_pipe != INVALID_HANDLE)
      return true;

   ulong now = GetTickCount64();
   if(now < g_next_retry_ms)
      return false;
   g_next_retry_ms = now + (ulong)MathMax(50, InpReconnectMs);

   if(g_token == "" && !LoadSessionToken())
      return false;

   ResetLastError();
   g_pipe = FileOpen(InpPipeName, FILE_READ|FILE_WRITE|FILE_BIN|FILE_ANSI);
   if(g_pipe == INVALID_HANDLE)
      return false;
   return true;
  }

void UpdateStatus()
  {
   if(!InpShowStatus)
      return;
   string state = (g_pipe != INVALID_HANDLE ? "CONNECTED" : "WAITING");
   Comment("BrainTrade Sniper Native: ", state,
           "\nSymbol: ", _Symbol,
           "\nTicks sent: ", (string)g_sent_ticks,
           "\nSend failures: ", (string)g_failed_ticks);
  }

int OnInit()
  {
   EventSetMillisecondTimer((int)MathMax(100, InpReconnectMs));
   EnsurePipe();
   UpdateStatus();
   return INIT_SUCCEEDED;
  }

void OnDeinit(const int reason)
  {
   EventKillTimer();
   ClosePipe();
   if(InpShowStatus)
      Comment("");
  }

void OnTimer()
  {
   EnsurePipe();
   UpdateStatus();
  }

void OnTick()
  {
   MqlTick tick;
   if(!SymbolInfoTick(_Symbol, tick))
      return;
   if(tick.time_msc <= 0 || tick.bid <= 0.0 || tick.ask < tick.bid)
      return;
   if(!EnsurePipe())
     {
      g_failed_ticks++;
      UpdateStatus();
      return;
     }

   long login = AccountInfoInteger(ACCOUNT_LOGIN);
   string packet = StringFormat("BT1|%s|%I64d|%s|%I64d|%.10f|%.10f\n",
                                g_token, login, _Symbol, (long)tick.time_msc,
                                tick.bid, tick.ask);
   ResetLastError();
   uint written = FileWriteString(g_pipe, packet);
   if(written == 0)
     {
      g_failed_ticks++;
      ClosePipe();
      UpdateStatus();
      return;
     }

   // Named-pipe writes are buffered by MQL5. Flush each Sniper tick so the
   // local gateway can wake immediately instead of waiting for the file cache.
   FileFlush(g_pipe);
   g_sent_ticks++;
   UpdateStatus();
  }
