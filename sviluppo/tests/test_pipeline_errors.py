"""Offline regression: caught provider failures must not become fake success."""
import importlib.util
from pathlib import Path
import unittest

path=Path(__file__).resolve().parents[1]/"bridge/clipsai_bridge.py"
spec=importlib.util.spec_from_file_location("flow_error_bridge",path)
bridge=importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class OutcomeTests(unittest.TestCase):
    def setUp(self):bridge.PROVIDER_FAILURES.clear()

    def test_legitimate_empty_proposals_are_not_errors(self):
        result={"clips":[]}
        self.assertIs(bridge.validate_provider_outcome(result,"clips"),result)

    def test_swallowed_error_and_no_candidates_is_failed(self):
        bridge.PROVIDER_FAILURES.append("fixture provider offline")
        for operation in ("clips","reels","shorts"):
            with self.assertRaisesRegex(bridge.ProviderError,"provider LLM ha fallito"):
                bridge.validate_provider_outcome({operation:[]},operation)

    def test_partial_analysis_preserves_candidates_with_visible_warning(self):
        bridge.PROVIDER_FAILURES.extend(["chunk two failed","chunk two failed"])
        value=bridge.validate_provider_outcome({"clips":[{"start":0,"end":25}]},"clips")
        self.assertEqual(value["_nl_suite_provider_warnings"],["chunk two failed"])
        self.assertEqual(len(value["clips"]),1)

    def test_deterministic_cleanup_is_available_but_not_silently_certified(self):
        bridge.PROVIDER_FAILURES.append("bad-take model failed")
        value=bridge.validate_provider_outcome({"good":[[0,1]]},"cleanup")
        self.assertEqual(value["good"],[[0,1]])
        self.assertEqual(value["_nl_suite_provider_warnings"],["bad-take model failed"])


if __name__=="__main__":unittest.main()
