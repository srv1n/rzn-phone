#!/usr/bin/env python3
from __future__ import annotations

import unittest
from collections import OrderedDict
from pathlib import Path
from unittest import mock

import build_bundle
import validate_workflow_catalog as catalog


def base_workflow() -> dict:
    return {
        "name": "test.workflow",
        "version": "1.0.0",
        "required_variables": ["query"],
        "capability": {
            "family": "extract",
            "intent": "search_results",
            "surface": "web",
            "mutating": False,
        },
        "inputs": {
            "query": {"type": "string", "required": True},
        },
        "steps": [
            {
                "tool": "ios.web.wait_js",
                "arguments": {"script": "return Boolean({{query}});"},
                "saveAs": "ready",
            },
            {
                "tool": "util.list.length",
                "when": {"var": "query", "truthy": True},
                "arguments": {"list": "{{steps.ready.value}}"},
                "saveAs": "count",
            },
        ],
        "output": {"count": "{{steps.count.count}}"},
        "presentation": {"cli": {"title": "{{query}}"}},
    }


class WorkflowCatalogValidatorTest(unittest.TestCase):
    def shape_errors(self, workflow: dict) -> list[str]:
        errors: list[str] = []
        catalog.validate_workflow_shape(
            Path("test_workflow.json"),
            workflow,
            {"ios.web.wait_js", "util.list.length"},
            errors,
        )
        return errors

    def test_accepts_engine_supported_workflow_shape(self) -> None:
        errors: list[str] = []
        workflow = base_workflow()
        catalog.validate_json_schema([(Path("test_workflow.json"), workflow)], errors)
        catalog.validate_workflow_shape(
            Path("test_workflow.json"),
            workflow,
            {"ios.web.wait_js", "util.list.length"},
            errors,
        )
        self.assertEqual(errors, [])

    def test_rejects_unknown_tools_missing_inputs_and_bad_capability(self) -> None:
        workflow = base_workflow()
        workflow["required_variables"] = ["missing"]
        workflow["capability"] = {
            "family": "bad",
            "intent": "",
            "surface": "",
            "mutating": "no",
        }
        workflow["steps"][0]["tool"] = "ios.missing_tool"

        errors = self.shape_errors(workflow)

        self.assertTrue(any("unknown tool" in error for error in errors))
        self.assertTrue(any("required variable 'missing'" in error for error in errors))
        self.assertTrue(any("invalid capability.family" in error for error in errors))
        self.assertTrue(any("capability.intent" in error for error in errors))
        self.assertTrue(any("capability.surface" in error for error in errors))
        self.assertTrue(any("capability.mutating" in error for error in errors))

    def test_rejects_unresolved_step_templates(self) -> None:
        workflow = base_workflow()
        workflow["steps"][0]["arguments"] = {"script": "{{steps.future.value}}"}
        workflow["output"] = {"missing": "{{steps.nope.value}}"}

        errors = self.shape_errors(workflow)

        self.assertTrue(any("{{steps.future.value}}" in error for error in errors))
        self.assertTrue(any("{{steps.nope.value}}" in error for error in errors))

    def test_catalog_metadata_detects_duplicate_ids_and_bundle_drift(self) -> None:
        workflow = base_workflow()
        errors: list[str] = []
        with mock.patch.object(
            catalog,
            "read_json",
            return_value={"version": catalog.load_cargo_version(), "payloads": []},
        ):
            catalog.validate_catalog_metadata(
                [
                    (Path("test_one.json"), workflow),
                    (Path("test_two.json"), workflow),
                ],
                errors,
            )

        self.assertTrue(any("duplicate workflow id" in error for error in errors))
        self.assertTrue(
            any("bundle payloads" in error for error in errors)
        )

    def test_bundle_resources_are_derived_from_packaged_payloads(self) -> None:
        manifest = build_bundle.build_manifest(
            {"id": "test", "version": "1.0.0", "name": "Test", "workers": []},
            "macos_arm64",
            OrderedDict(
                [
                    ("resources/workflows/test.json", "a"),
                    ("resources/systems/test.yaml", "b"),
                    ("skills/test.md", "c"),
                ]
            ),
        )

        self.assertEqual(
            manifest["resources"],
            [
                {"path": "resources/workflows/test.json"},
                {"path": "resources/systems/test.yaml"},
            ],
        )


if __name__ == "__main__":
    unittest.main()
