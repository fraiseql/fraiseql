# Conftest for tests/chaos/concurrency
import sys
import os
tests_dir = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if tests_dir not in sys.path:
    sys.path.insert(0, tests_dir)
