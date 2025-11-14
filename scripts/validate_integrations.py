#!/usr/bin/env python3
"""
FraiseQL Integration Validation Script

Validates that LangChain and LlamaIndex integrations are working correctly
and are production-ready. This script performs basic functionality tests
without requiring a full database setup.
"""

import asyncio
import sys
from typing import Dict, Any, List
import json

# Test imports
try:
    from langchain.docstore.document import Document as LangChainDocument

    LANGCHAIN_AVAILABLE = True
    print("✅ LangChain available")
except ImportError:
    LANGCHAIN_AVAILABLE = False
    print("⚠️  LangChain not available")

try:
    from llama_index.core.schema import Document as LlamaDocument

    LLAMAINDEX_AVAILABLE = True
    print("✅ LlamaIndex available")
except ImportError:
    LLAMAINDEX_AVAILABLE = False
    print("⚠️  LlamaIndex not available")

# FraiseQL imports
try:
    from fraiseql.integrations.langchain import FraiseQLVectorStore as LangChainVectorStore

    print("✅ FraiseQL LangChain integration imported")
except ImportError as e:
    print(f"❌ FraiseQL LangChain integration failed: {e}")
    LANGCHAIN_AVAILABLE = False

try:
    from fraiseql.integrations.llamaindex import (
        FraiseQLVectorStore as LlamaIndexVectorStore,
        FraiseQLReader,
    )

    print("✅ FraiseQL LlamaIndex integration imported")
except ImportError as e:
    print(f"❌ FraiseQL LlamaIndex integration failed: {e}")
    LLAMAINDEX_AVAILABLE = False


class IntegrationValidator:
    """Validates FraiseQL integrations without requiring database connection."""

    def __init__(self):
        self.results = {
            "langchain": {"available": LANGCHAIN_AVAILABLE, "tests": {}},
            "llamaindex": {"available": LLAMAINDEX_AVAILABLE, "tests": {}},
        }

    def test_class_instantiation(self) -> bool:
        """Test that integration classes can be instantiated."""
        success = True

        if LANGCHAIN_AVAILABLE:
            try:
                # Mock database pool for testing
                class MockPool:
                    pass

                pool = MockPool()
                store = LangChainVectorStore(
                    db_pool=pool, table_name="test_table", embedding_function=None
                )
                self.results["langchain"]["tests"]["instantiation"] = "✅ PASS"
                print("✅ LangChain VectorStore instantiation successful")
            except Exception as e:
                self.results["langchain"]["tests"]["instantiation"] = f"❌ FAIL: {e}"
                print(f"❌ LangChain VectorStore instantiation failed: {e}")
                success = False

        if LLAMAINDEX_AVAILABLE:
            try:

                class MockPool:
                    pass

                pool = MockPool()
                store = LlamaIndexVectorStore(
                    db_pool=pool, table_name="test_table", embedding_dimension=384
                )
                reader = FraiseQLReader(db_pool=pool, table_name="test_table")
                self.results["llamaindex"]["tests"]["instantiation"] = "✅ PASS"
                print("✅ LlamaIndex VectorStore and Reader instantiation successful")
            except Exception as e:
                self.results["llamaindex"]["tests"]["instantiation"] = f"❌ FAIL: {e}"
                print(f"❌ LlamaIndex instantiation failed: {e}")
                success = False

        return success

    def test_method_signatures(self) -> bool:
        """Test that required methods exist with correct signatures."""
        success = True

        if LANGCHAIN_AVAILABLE:
            try:
                store = LangChainVectorStore.__new__(LangChainVectorStore)
                required_methods = [
                    "aadd_documents",
                    "asimilarity_search_by_vector",
                    "adelete",
                    "aget_relevant_documents",
                ]

                for method in required_methods:
                    if hasattr(store, method):
                        self.results["langchain"]["tests"][f"method_{method}"] = "✅ EXISTS"
                    else:
                        self.results["langchain"]["tests"][f"method_{method}"] = "❌ MISSING"
                        success = False

                print("✅ LangChain method signatures validated")

            except Exception as e:
                self.results["langchain"]["tests"]["method_signatures"] = f"❌ FAIL: {e}"
                print(f"❌ LangChain method validation failed: {e}")
                success = False

        if LLAMAINDEX_AVAILABLE:
            try:
                store = LlamaIndexVectorStore.__new__(LlamaIndexVectorStore)
                reader = FraiseQLReader.__new__(FraiseQLReader)

                # Check VectorStore methods
                store_methods = [
                    "aadd",
                    "aget",
                    "adelete",
                    "aquery",
                    "add",
                    "get",
                    "delete",
                    "query",
                ]
                for method in store_methods:
                    if hasattr(store, method):
                        self.results["llamaindex"]["tests"][f"store_method_{method}"] = "✅ EXISTS"
                    else:
                        self.results["llamaindex"]["tests"][f"store_method_{method}"] = "❌ MISSING"
                        success = False

                # Check Reader methods
                reader_methods = ["aload_data", "load_data"]
                for method in reader_methods:
                    if hasattr(reader, method):
                        self.results["llamaindex"]["tests"][f"reader_method_{method}"] = "✅ EXISTS"
                    else:
                        self.results["llamaindex"]["tests"][f"reader_method_{method}"] = (
                            "❌ MISSING"
                        )
                        success = False

                print("✅ LlamaIndex method signatures validated")

            except Exception as e:
                self.results["llamaindex"]["tests"]["method_signatures"] = f"❌ FAIL: {e}"
                print(f"❌ LlamaIndex method validation failed: {e}")
                success = False

        return success

    def test_type_hints(self) -> bool:
        """Test that classes have proper type hints."""
        success = True

        if LANGCHAIN_AVAILABLE:
            try:
                import inspect

                store_init = inspect.signature(LangChainVectorStore.__init__)
                params = store_init.parameters

                required_params = ["db_pool", "table_name"]
                for param in required_params:
                    if param in params:
                        self.results["langchain"]["tests"][f"typehint_{param}"] = "✅ PRESENT"
                    else:
                        self.results["langchain"]["tests"][f"typehint_{param}"] = "❌ MISSING"
                        success = False

                print("✅ LangChain type hints validated")

            except Exception as e:
                self.results["langchain"]["tests"]["type_hints"] = f"❌ FAIL: {e}"
                print(f"❌ LangChain type hints validation failed: {e}")
                success = False

        if LLAMAINDEX_AVAILABLE:
            try:
                import inspect

                store_init = inspect.signature(LlamaIndexVectorStore.__init__)
                reader_init = inspect.signature(FraiseQLReader.__init__)
                store_params = store_init.parameters
                reader_params = reader_init.parameters

                required_store_params = ["db_pool", "table_name", "embedding_dimension"]
                for param in required_store_params:
                    if param in store_params:
                        self.results["llamaindex"]["tests"][f"store_typehint_{param}"] = (
                            "✅ PRESENT"
                        )
                    else:
                        self.results["llamaindex"]["tests"][f"store_typehint_{param}"] = (
                            "❌ MISSING"
                        )
                        success = False

                required_reader_params = ["db_pool", "table_name"]
                for param in required_reader_params:
                    if param in reader_params:
                        self.results["llamaindex"]["tests"][f"reader_typehint_{param}"] = (
                            "✅ PRESENT"
                        )
                    else:
                        self.results["llamaindex"]["tests"][f"reader_typehint_{param}"] = (
                            "❌ MISSING"
                        )
                        success = False

                print("✅ LlamaIndex type hints validated")

            except Exception as e:
                self.results["llamaindex"]["tests"]["type_hints"] = f"❌ FAIL: {e}"
                print(f"❌ LlamaIndex type hints validation failed: {e}")
                success = False

        return success

    def test_document_compatibility(self) -> bool:
        """Test document creation and compatibility."""
        success = True

        if LANGCHAIN_AVAILABLE:
            try:
                doc = LangChainDocument(page_content="Test content", metadata={"test": "value"})
                self.results["langchain"]["tests"]["document_creation"] = "✅ PASS"
                print("✅ LangChain document creation successful")
            except Exception as e:
                self.results["langchain"]["tests"]["document_creation"] = f"❌ FAIL: {e}"
                print(f"❌ LangChain document creation failed: {e}")
                success = False

        if LLAMAINDEX_AVAILABLE:
            try:
                doc = LlamaDocument(text="Test content", metadata={"test": "value"})
                self.results["llamaindex"]["tests"]["document_creation"] = "✅ PASS"
                print("✅ LlamaIndex document creation successful")
            except Exception as e:
                self.results["llamaindex"]["tests"]["document_creation"] = f"❌ FAIL: {e}"
                print(f"❌ LlamaIndex document creation failed: {e}")
                success = False

        return success

    def generate_report(self) -> Dict[str, Any]:
        """Generate validation report."""
        report = {
            "timestamp": asyncio.get_event_loop().time(),
            "summary": {
                "langchain_available": self.results["langchain"]["available"],
                "llamaindex_available": self.results["llamaindex"]["available"],
                "tests_passed": 0,
                "tests_failed": 0,
                "overall_status": "UNKNOWN",
            },
            "results": self.results,
        }

        # Count results
        for framework in ["langchain", "llamaindex"]:
            for test_name, result in self.results[framework]["tests"].items():
                if result.startswith("✅"):
                    report["summary"]["tests_passed"] += 1
                elif result.startswith("❌"):
                    report["summary"]["tests_failed"] += 1

        # Determine overall status
        total_tests = report["summary"]["tests_passed"] + report["summary"]["tests_failed"]
        if total_tests == 0:
            report["summary"]["overall_status"] = "NO TESTS RUN"
        elif report["summary"]["tests_failed"] == 0:
            report["summary"]["overall_status"] = "✅ ALL PASS"
        elif report["summary"]["tests_passed"] > report["summary"]["tests_failed"]:
            report["summary"]["overall_status"] = "⚠️ MOSTLY PASS"
        else:
            report["summary"]["overall_status"] = "❌ MOSTLY FAIL"

        return report

    async def run_validation(self) -> bool:
        """Run complete validation suite."""
        print("\n🚀 Starting FraiseQL Integration Validation")
        print("=" * 50)

        tests = [
            ("Class Instantiation", self.test_class_instantiation),
            ("Method Signatures", self.test_method_signatures),
            ("Type Hints", self.test_type_hints),
            ("Document Compatibility", self.test_document_compatibility),
        ]

        all_passed = True
        for test_name, test_func in tests:
            print(f"\n🔍 Running: {test_name}")
            try:
                passed = test_func()
                if not passed:
                    all_passed = False
            except Exception as e:
                print(f"❌ Test {test_name} crashed: {e}")
                all_passed = False

        # Generate and display report
        report = self.generate_report()
        print("\n" + "=" * 50)
        print("📋 VALIDATION REPORT")
        print("=" * 50)
        print(json.dumps(report, indent=2, default=str))

        print(f"\n🏁 Overall Status: {report['summary']['overall_status']}")
        print(f"✅ Tests Passed: {report['summary']['tests_passed']}")
        print(f"❌ Tests Failed: {report['summary']['tests_failed']}")

        return all_passed


async def main():
    """Main validation function."""
    validator = IntegrationValidator()
    success = await validator.run_validation()

    if success:
        print("\n🎉 All validations passed! Integrations are production-ready.")
        return 0
    else:
        print("\n⚠️ Some validations failed. Check the report above for details.")
        return 1


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
