import json
import re
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONSTRUCT = ROOT / "packaging" / "constructor" / "construct.yaml"
MENU = ROOT / "packaging" / "constructor" / "menu" / "cq-editor_menu.json"
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"


class ReleasePackagingTests(unittest.TestCase):
    def test_release_version_is_synchronized(self):
        cargo_version = tomllib.loads(
            (ROOT / "Cargo.toml").read_text(encoding="utf-8")
        )["package"]["version"]
        worker_version = tomllib.loads(
            (ROOT / "worker" / "pyproject.toml").read_text(encoding="utf-8")
        )["project"]["version"]
        worker_init = (ROOT / "worker" / "cq_worker" / "__init__.py").read_text(
            encoding="utf-8"
        )
        construct = CONSTRUCT.read_text(encoding="utf-8")
        menu = json.loads(MENU.read_text(encoding="utf-8"))

        self.assertEqual(cargo_version, "0.8.0")
        self.assertEqual(worker_version, cargo_version)
        self.assertIn(f'__version__ = "{cargo_version}"', worker_init)
        self.assertRegex(construct, rf"(?m)^version: {re.escape(cargo_version)}$")
        self.assertEqual(
            menu["menu_items"][0]["platforms"]["osx"]["CFBundleVersion"],
            cargo_version,
        )
        self.assertIn(
            f"## {cargo_version}", (ROOT / "docs" / "changelog.md").read_text()
        )

    def test_installer_contains_a_pinned_cadquery_runtime(self):
        construct = CONSTRUCT.read_text(encoding="utf-8")
        for package in (
            "python=3.12.15",
            "cadquery=2.8.0",
            "ocp=7.9.3.1",
            "jedi=0.19.2",
            "menuinst=2.5.2",
            "nomkl",
        ):
            self.assertIn(f"  - {package}", construct)
        self.assertIn("target/release/cq-editor: bin/cq-editor", construct)
        self.assertIn("target/release/cq-editor.exe: cq-editor.exe", construct)
        self.assertIn("lockfile", construct)
        self.assertIn("licenses", construct)

    def test_native_shortcuts_launch_the_bundled_executable(self):
        construct = CONSTRUCT.read_text(encoding="utf-8")
        workflow = WORKFLOW.read_text(encoding="utf-8")
        menu = json.loads(MENU.read_text(encoding="utf-8"))
        item = menu["menu_items"][0]
        self.assertFalse(item["activate"])
        self.assertEqual(
            item["platforms"]["win"]["command"],
            ["{{ PREFIX }}/cq-editor.exe"],
        )
        self.assertEqual(item["platforms"]["win"]["file_extensions"], [".py"])
        self.assertEqual(
            item["platforms"]["linux"]["command"][-1],
            "%F",
        )
        self.assertIn("text/x-python", item["platforms"]["linux"]["MimeType"])
        self.assertEqual(
            item["platforms"]["osx"]["CFBundleIdentifier"],
            "io.github.igors93.cq-editor",
        )

        # Constructor rejects macOS PKG prefixes with spaces because its
        # embedded conda runtime cannot be safely relocated into such paths.
        mac_location = next(
            line.split(":", 1)[1].split("#", 1)[0].strip()
            for line in construct.splitlines()
            if line.startswith("default_location_pkg:")
        )
        self.assertNotIn(" ", mac_location)
        self.assertIn('prefix="$HOME/Library/CQ-editor"', workflow)

    def test_release_workflow_builds_and_smoke_tests_every_installer(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        for platform in (
            "linux-x86_64",
            "windows-x86_64",
            "macos-arm64",
            "macos-x86_64",
        ):
            self.assertIn(f"platform: {platform}", workflow)
        self.assertIn("constructor=3.17.3", workflow)
        self.assertIn("Name build manifests for the platform", workflow)
        self.assertIn("Smoke test installed runtime", workflow)
        self.assertIn("softprops/action-gh-release@v2", workflow)
        self.assertIn("SHA256SUMS.txt", workflow)

    def test_packaging_scripts_and_static_inputs_exist(self):
        for relative in (
            "packaging/constructor/post_install.sh",
            "packaging/constructor/post_install.bat",
            "packaging/constructor/pre_uninstall.bat",
            "assets/icons/cadquery_logo_dark.svg",
            "assets/icons/cadquery_logo_dark.ico",
            "LICENSE",
        ):
            self.assertTrue((ROOT / relative).is_file(), relative)
        self.assertIn(
            "--install cq-editor_menu.json",
            (ROOT / "packaging" / "constructor" / "post_install.sh").read_text(),
        )


if __name__ == "__main__":
    unittest.main()
