"""Deterministic chunk ownership checks, without loading an ASR model."""
import importlib.util
from pathlib import Path
import unittest
import tempfile
import json

MODULE = Path(__file__).resolve().parents[1] / 'bridge/qwen_asr_bridge.py'
spec = importlib.util.spec_from_file_location('qwen_bridge', MODULE)
bridge = importlib.util.module_from_spec(spec); spec.loader.exec_module(bridge)


class ChunkTests(unittest.TestCase):
    def test_zero_duration_text_retained_separately_without_fake_word_times(self):
        with tempfile.TemporaryDirectory(prefix='nl-qwen-diagnostics-') as directory:
            cache = Path(directory)
            (cache/'silences.json').write_text('[]')
            raw = {'words':[{'word':'Caffè','start':.1,'end':.3},
                            {'word':'leggenda.','start':.4,'end':.4}]}
            (cache/'chunk-00000.json').write_text(json.dumps(raw))
            original = (cache/'chunk-00000.json').read_bytes()
            value = bridge.alignment_diagnostics(cache, 1000, 30)
            self.assertEqual(value['unplaced_word_count'], 1)
            self.assertEqual(value['raw_owned_chunk_texts'][0]['text'], 'Caffè leggenda.')
            self.assertEqual(value['unplaced_words'][0]['at_ms'], 400)
            self.assertNotIn('end_ms', value['unplaced_words'][0])
            self.assertFalse(value['unplaced_words'][0]['timing_verified'])
            self.assertEqual((cache/'chunk-00000.json').read_bytes(), original)
            self.assertEqual(bridge.alignment_diagnostics(cache,1000,30),value)

    def test_long_audio_is_bounded_and_joins_at_real_pauses(self):
        plan = bridge.chunk_plan(7_200_000, [[28_000, 30_000]], 30)
        self.assertEqual(plan[0]['core_end'], 29_000)
        self.assertLessEqual(max(c['end']-c['start'] for c in plan), 31_500)
        for left, right in zip(plan, plan[1:]):
            self.assertEqual(left['core_end'], right['core_start'])
            self.assertLess(right['start'], left['end'])

    def test_silence_is_skipped_but_repeated_words_are_preserved(self):
        self.assertEqual(bridge.chunk_plan(30_000, [[0, 30_000]], 30), [])
        chunk = {'core_start':0, 'core_end':3000, 'start':0, 'end':3000}
        words = [{'word':'no', 'start':.1, 'end':.3}, {'word':'no', 'start':.4, 'end':.6}]
        merged = bridge.merge_words([(chunk, words)])
        self.assertEqual([w['text'] for w in merged], ['no', 'no'])

    def test_context_ownership_removes_overlap_without_duplicating_offsets(self):
        left = {'core_start':0, 'core_end':1000, 'start':0, 'end':1750}
        right = {'core_start':1000, 'core_end':2000, 'start':250, 'end':2000}
        merged = bridge.merge_words([(left,[{'word':'uno','start':.5,'end':.7},{'word':'due','start':1.1,'end':1.3}]), (right,[{'word':'uno','start':.25,'end':.45},{'word':'due','start':.85,'end':1.05}])])
        self.assertEqual([w['text'] for w in merged], ['uno','due'])
        self.assertEqual(merged[1]['start_ms'], 1100)


if __name__ == '__main__': unittest.main()
