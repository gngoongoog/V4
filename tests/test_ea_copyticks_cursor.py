"""Model MT5 CopyTicks' inclusive, oldest-first cursor without MT5 or trades.

The vendor documents that a positive `from` counts forward and `count` limits
the returned rows. This model tests the intended recovery geometry; only
MetaEditor and an actual terminal can validate the compiled EA itself.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
EA = (ROOT / 'mt5/BrainTradeSniperBridge.mq5').read_text('utf-8')
EVENT_FACTOR = 1_000_000


def model_batch(ticks, old_ms, old_count, current_ms, batch, limit=65536):
    """Return a bounded model batch and monotonic event IDs."""
    from_ms = current_ms if old_ms == 0 or current_ms-old_ms > 1000 else old_ms
    if from_ms == current_ms and (old_ms == 0 or current_ms-old_ms > 1000):
        old_ms = old_count = 0
    saturated = False
    if old_ms > 0 and old_count >= limit:
        if current_ms <= old_ms:
            return old_ms, old_count, [], False, False
        from_ms, old_ms, old_count, saturated = old_ms+1, 0, 0, True
    request = min(limit, batch+old_count)
    rows = [ms for ms in ticks if ms >= from_ms][:request]
    new_ms, new_count, seen_old = old_ms, old_count, 0
    emitted = []
    for ms in rows:
        if old_ms > 0 and ms == old_ms:
            seen_old += 1
            if seen_old <= old_count:
                continue
            ordinal = seen_old
            new_ms, new_count = ms, ordinal
        else:
            if ms < old_ms:
                continue
            if ms != new_ms:
                new_ms, new_count = ms, 0
            new_count += 1
            ordinal = new_count
        emitted.append(ms*EVENT_FACTOR+ordinal)
    progressed = new_ms > old_ms or (new_ms == old_ms and new_count > old_count)
    pending = progressed and (len(rows) == request or current_ms > new_ms)
    return new_ms, new_count, emitted, pending, saturated


class CopyTicksRecoveryTests(unittest.TestCase):
    def test_same_millisecond_full_batch_makes_progress_without_duplicates(self):
        ticks = [1000]*40 + [1001]*3
        ms, count, first, pending, _ = model_batch(ticks,0,0,1000,32)
        self.assertEqual((ms,count,len(first),pending),(1000,32,32,True))
        # The old fixed count of 32 returns only already-consumed rows.
        self.assertEqual([t for t in ticks if t >= 1000][:32], [1000]*32)
        ms,count,second,pending,_ = model_batch(ticks,ms,count,1001,32)
        all_ids = first+second
        self.assertEqual(len(second),11)
        self.assertEqual(len(all_ids),43)
        self.assertEqual(all_ids,sorted(set(all_ids)))
        self.assertEqual((ms,count,pending),(1001,3,False))

    def test_bounded_saturation_is_visible_and_moves_only_to_new_millisecond(self):
        ticks=[1000]*70+[1001]*2
        ms,count,values,pending,saturated=model_batch(ticks,1000,64,1000,32,64)
        self.assertEqual((ms,count,values,pending,saturated),(1000,64,[],False,False))
        ms,count,values,pending,saturated=model_batch(ticks,1000,64,1001,32,64)
        self.assertTrue(saturated)
        self.assertEqual(values,[1001*EVENT_FACTOR+1,1001*EVENT_FACTOR+2])
        self.assertEqual((ms,count,pending),(1001,2,False))

    def test_ea_wires_bounded_cursor_and_timer_without_trading(self):
        self.assertIn('#define MAX_COPY_REQUEST 65536',EA)
        self.assertRegex(EA,r'CopyTicks\(_Symbol,ticks,COPY_TICKS_ALL,from_msc,request_count\)')
        self.assertIn('(double)batch+(double)old_count',EA)
        self.assertIn('g_saturated_milliseconds++',EA)
        timer=re.search(r'void OnTimer\(\)\s*\{(.*?)\n  \}',EA,re.S)
        self.assertIsNotNone(timer)
        self.assertIn('EnsurePipe() && g_pending_batch',timer.group(1))
        self.assertIn('SendPendingQuoteTicks()',timer.group(1))
        self.assertNotIn('OrderSend(',EA)


if __name__ == '__main__':
    unittest.main()
