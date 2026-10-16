//+------------------------------------------------------------------+
//| BrainTradeSniperBridge.mq5                                      |
//| Native tick transport only. It never submits broker orders.     |
//+------------------------------------------------------------------+
#property strict
#property version   "1.10"
#property description "BrainTrade native Sniper tick bridge. Transport only; no trading calls."

input string InpPipeName = "\\\\.\\pipe\\BrainTradeSniperV1";
input int    InpReconnectMs = 250;
input int    InpMaxBatch = 2048;
input bool   InpShowStatus = true;

#define TOKEN_FILE "BrainTrade\\sniper.token"
#define EVENT_FACTOR 1000000

int    g_pipe = INVALID_HANDLE;
string g_token = "";
ulong  g_next_retry_ms = 0;
ulong  g_sent_ticks = 0;
ulong  g_recovered_ticks = 0;
ulong  g_failed_ticks = 0;
long   g_cursor_msc = 0;
uint   g_cursor_count = 0;

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

   // Reload after every reconnect. The gateway rotates the token when it binds
   // a new MT5 session/data folder.
   g_token = "";
   if(!LoadSessionToken())
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
           "\nQuote ticks sent: ", (string)g_sent_ticks,
           "\nRecovered in batches: ", (string)g_recovered_ticks,
           "\nSend failures: ", (string)g_failed_ticks);
  }

ulong TickEventId(const long time_msc,const uint ordinal)
  {
   if(time_msc <= 0 || ordinal >= EVENT_FACTOR)
      return 0;
   return ((ulong)time_msc * (ulong)EVENT_FACTOR) + (ulong)ordinal;
  }

bool AppendQuotePacket(string &payload,const MqlTick &tick,const uint ordinal,const long login)
  {
   if(tick.time_msc <= 0 || tick.bid <= 0.0 || tick.ask < tick.bid)
      return false;
   // Sniper consumes executable quote changes. Pure Last/volume ticks are
   // advanced in the cursor but are not forwarded as duplicate Bid/Ask states.
   if((tick.flags & (TICK_FLAG_BID|TICK_FLAG_ASK)) == 0)
      return false;
   ulong event_id = TickEventId((long)tick.time_msc,ordinal);
   if(event_id == 0)
      return false;
   payload += StringFormat("BT2|%s|%I64d|%s|%I64d|%I64u|%.10f|%.10f\n",
                           g_token,login,_Symbol,(long)tick.time_msc,event_id,
                           tick.bid,tick.ask);
   return true;
  }

bool SendPendingQuoteTicks()
  {
   if(!EnsurePipe())
      return false;

   MqlTick current;
   if(!SymbolInfoTick(_Symbol,current) || current.time_msc <= 0)
      return false;

   // Never replay old market history into a live decision engine. On first
   // attach, or after a gap longer than the Core's 1s Sniper freshness window,
   // resume from the current millisecond only. During normal operation CopyTicks
   // recovers all terminal-cached ticks since our last cursor.
   long old_msc = g_cursor_msc;
   uint old_count = g_cursor_count;
   ulong from_msc;
   if(old_msc <= 0 || current.time_msc < old_msc || current.time_msc-old_msc > 1000)
     {
      from_msc = (ulong)current.time_msc;
      old_msc = 0;
      old_count = 0;
     }
   else
      from_msc = (ulong)old_msc;

   int batch = (int)MathMax(32,MathMin(4096,InpMaxBatch));
   MqlTick ticks[];
   ResetLastError();
   int copied = CopyTicks(_Symbol,ticks,COPY_TICKS_ALL,from_msc,(uint)batch);
   if(copied <= 0)
      return false;

   long login = AccountInfoInteger(ACCOUNT_LOGIN);
   if(login <= 0)
      return false;

   string payload = "";
   long new_msc = old_msc;
   uint new_count = old_count;
   uint seen_old = 0;
   uint forwarded = 0;

   // CopyTicks returns source ticks from oldest to newest. The cursor counts all
   // ticks at the final millisecond so an inclusive next request can skip exactly
   // what was already consumed, even when several ticks share one time_msc.
   for(int i=0;i<copied;i++)
     {
      long tms = (long)ticks[i].time_msc;
      if(tms <= 0)
         continue;
      uint ordinal = 0;
      if(old_msc > 0 && tms == old_msc)
        {
         seen_old++;
         if(seen_old <= old_count)
            continue;
         ordinal = seen_old;
         new_msc = tms;
         new_count = ordinal;
        }
      else
        {
         if(tms < old_msc)
            continue;
         if(tms != new_msc)
           {
            new_msc = tms;
            new_count = 0;
           }
         new_count++;
         ordinal = new_count;
        }

      if(AppendQuotePacket(payload,ticks[i],ordinal,login))
         forwarded++;
     }

   // Cursor advancement is committed only after the corresponding quote batch
   // was fully accepted into the pipe buffer. With FILE_ANSI the documented
   // FileWriteString byte count equals this ASCII payload's character count, so
   // a short write is treated exactly like a failed write and retried from the
   // unchanged terminal-cache cursor.
   if(forwarded > 0)
     {
      ResetLastError();
      uint expected = (uint)StringLen(payload);
      uint written = FileWriteString(g_pipe,payload);
      if(written != expected)
        {
         g_failed_ticks += forwarded;
         ClosePipe();
         return false;
        }
      // One flush per recovered batch, not one flush per source tick. This keeps
      // the OnTick handler short while waking the local reader immediately.
      FileFlush(g_pipe);
      g_sent_ticks += forwarded;
      if(forwarded > 1)
         g_recovered_ticks += forwarded-1;
     }

   if(new_msc > 0)
     {
      g_cursor_msc = new_msc;
      g_cursor_count = new_count;
     }
   return true;
  }

int OnInit()
  {
   EventSetMillisecondTimer((int)MathMax(100,InpReconnectMs));
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
   if(!SendPendingQuoteTicks())
      g_failed_ticks++;
   UpdateStatus();
  }
