"""Offline replay of a real accepted, module-backed MCP delivery.

This deliberately uses the public MCP protocol and verified stream. Once the
delivery is downloaded, both the source workspace and durable agent state are
removed from the replay path.
"""

import copy
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from agent_drive_demo import Client, run  # noqa: E402
from replay_delivery import replay_accepted  # noqa: E402


class AcceptedOfflineReplay(unittest.TestCase):
    def test_locked_module_selected_output_and_overrides_replay_without_agent(self):
        with tempfile.TemporaryDirectory(prefix="cad-accepted-replay-") as temporary:
            root = Path(temporary)
            cad = root / "cad"
            shutil.copy2(ROOT / "target/release/cad", cad)
            executable_sha256 = hashlib.sha256(cad.read_bytes()).hexdigest()

            workspace = root / "workspace"
            examples = workspace / "examples"
            modules = examples / "recipe_modules"
            modules.mkdir(parents=True)
            for name in (
                "modular_linear_stage.star",
                "modular_linear_stage.star.lock.json",
            ):
                shutil.copy2(ROOT / "examples" / name, examples / name)
            for name in ("drive_parts.star", "stock.star"):
                shutil.copy2(ROOT / "examples/recipe_modules" / name, modules / name)
            recipe = examples / "modular_linear_stage.star"
            recipe.write_text(
                recipe.read_text()
                + '\ninspection = assembly(id = "inspection", instances = [instance(id = "single_rail", part = rail)])\n'
                + 'assembly_output(name = "unselected_inspection", assembly = inspection)\n'
            )

            state = root / "state"
            delivery = root / "delivery"
            client = Client(cad, workspace, state)
            try:
                captured = client.tool("cad_snapshot", {"recipe": "examples/modular_linear_stage.star"})
                job = client.tool(
                    "cad_evaluate",
                    {
                        "request_id": "modular-selected-172",
                        "snapshot_id": captured["snapshot_id"],
                        "mode": "build",
                        "outputs": ["modular_linear_stage"],
                        "params": {"length": 172, "bore": 3.5},
                    },
                )
                self.assertTrue(job["worker"]["success"], job)
                self.assertTrue(client.tool("cad_verify", {
                    "job_id": job["job_id"], "manifest_digest": job["manifest_hash"]
                })["success"])
                accepted = client.tool("cad_accept", {
                    "request_id": "accept-modular-selected-172",
                    "job_id": job["job_id"],
                    "manifest_digest": job["manifest_hash"],
                    "expected_base_revision": None,
                })
                self.assertTrue(accepted["accepted"])
                downloaded = client.download(job, delivery, stream=True)
                self.assertEqual(downloaded["mode"], "verified_generation_stream")
                self.assertGreater(downloaded["artifacts"], 5)
            finally:
                client.close()

            self.assertTrue(run(cad, "verify", delivery)["valid"])
            manifest = json.loads((delivery / "manifest.json").read_text())
            self.assertEqual(manifest["recipe_source"]["parameter_overrides"],
                             {"length": 172, "bore": 3.5})
            self.assertEqual(manifest["scope"]["requested_outputs"], ["modular_linear_stage"])
            self.assertIn("modular_linear_stage", manifest["outputs"])
            self.assertNotIn("unselected_inspection", manifest["outputs"])
            self.assertTrue((delivery / "recipe.bundle.json").is_file())
            self.assertTrue(manifest["property_assertions"])
            self.assertTrue(manifest["section_views"])

            workspace.rename(root / "workspace-offline")
            state.rename(root / "state-offline")
            self.assertFalse(workspace.exists())
            self.assertFalse(state.exists())
            replay = replay_accepted(cad, delivery, accepted, job, executable_sha256)
            self.assertTrue(replay["valid"])
            self.assertEqual(replay["accepted_revision_id"], accepted["revision"]["revision_id"])
            self.assertEqual(replay["source_kind"], "captured_locked_bundle")
            self.assertEqual(replay["parameter_overrides"], {"length": 172, "bore": 3.5})
            self.assertEqual(replay["scope"]["mode"], "selected")
            compared = {entry["name"] for entry in replay["compared_files"]}
            self.assertTrue(any(name.endswith(".brep.json") for name in compared))
            self.assertTrue(any(name.endswith(".step") for name in compared))
            self.assertTrue(any(name.startswith("section-") and name.endswith(".json")
                                for name in compared))

            altered_acceptance = copy.deepcopy(accepted)
            altered_acceptance["revision"]["manifest_digest"] = "0" * 64
            with self.assertRaises(ValueError):
                replay_accepted(cad, delivery, altered_acceptance, job, executable_sha256)
            altered_job = copy.deepcopy(job)
            altered_job["job_id"] = "job_0000000000009999"
            with self.assertRaises(ValueError):
                replay_accepted(cad, delivery, accepted, altered_job, executable_sha256)
            with self.assertRaises(ValueError):
                replay_accepted(cad, delivery, accepted, job, "0" * 64)

            tampered = root / "tampered-delivery"
            shutil.copytree(delivery, tampered)
            bundle = tampered / "recipe.bundle.json"
            contents = bundle.read_bytes()
            bundle.write_bytes(contents.replace(b"modular_linear_stage", b"modular_linear_stagE", 1))
            self.assertNotEqual(bundle.read_bytes(), contents)
            with self.assertRaises(ValueError):
                replay_accepted(cad, tampered, accepted, job, executable_sha256)

            for field, value in (("parameter_overrides", {"length": 160, "bore": 3.5}),
                                 ("requested_outputs", ["unselected_inspection"])):
                with self.subTest(manifest_field=field):
                    altered = root / f"tampered-{field}"
                    shutil.copytree(delivery, altered)
                    path = altered / "manifest.json"
                    changed = json.loads(path.read_text())
                    if field == "parameter_overrides":
                        changed["recipe_source"][field] = value
                    else:
                        changed["scope"][field] = value
                    path.write_text(json.dumps(changed))
                    with self.assertRaises(ValueError):
                        replay_accepted(cad, altered, accepted, job, executable_sha256)


if __name__ == "__main__":
    unittest.main()
