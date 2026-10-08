"""Run with python3 script/test_bundle_macos.py on macOS."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parent.parent


@unittest.skipUnless(sys.platform == "darwin", "requires macOS codesign")
class BundleMacOSTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="blink bundle test ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "script").mkdir()
        shutil.copy2(ROOT / "script/bundle-macos", self.root / "script/bundle-macos")
        resources = self.root / "crates/blink/resources"
        resources.mkdir(parents=True)
        for name in ("Info.plist", "icon.icns"):
            shutil.copy2(ROOT / "crates/blink/resources" / name, resources / name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        # Replace the slow compiler and the user's keychain. Bundle assembly,
        # version updates, signing, and signature verification remain real.
        self.tool("cargo", '''
case "$1" in
  pkgid) echo 'path+file:///fixture#blink@1.2.3' ;;
  build)
    touch build-called
    [ "${FAIL_BUILD:-0}" = 0 ] || exit 1
    mkdir -p target/release
    /usr/bin/clang -x c -o target/release/blink - <<'C'
int main(void) { return 0; }
C
    ;;
  *) exit 1 ;;
esac
''')
        self.tool("security", 'printf "%s\\n" "${TEST_IDENTITIES:-0 valid identities found}"')
        self.env = dict(os.environ, PATH=f"{self.bin}:/usr/bin:/bin:/usr/sbin:/sbin")
        for name in ("MACOS_SIGN_IDENTITY", "BLINK_UNIVERSAL", "NOTARY_PROFILE"):
            self.env.pop(name, None)
        self.app = self.root / "target/release/Blink.app"

    def tool(self, name, body):
        path = self.bin / name
        path.write_text("#!/bin/sh\nset -eu\n" + body + "\n")
        path.chmod(0o755)

    def run_bundle(self, *args):
        return subprocess.run(
            [str(self.root / "script/bundle-macos"), *args],
            cwd=self.root, env=self.env, text=True, capture_output=True,
        )

    def test_missing_certificate_fails_before_build_without_adhoc_fallback(self):
        result = self.run_bundle()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Developer ID Application", result.stderr)
        self.assertFalse((self.root / "build-called").exists())

    def test_multiple_certificates_require_an_explicit_choice(self):
        self.env["TEST_IDENTITIES"] = (
            '  1) ' + 'A' * 40 + ' "Developer ID Application: One (ONE)"\n'
            '  2) ' + 'B' * 40 + ' "Developer ID Application: Two (TWO)"\n'
            '     2 valid identities found'
        )
        result = self.run_bundle()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--identity", result.stderr)
        self.assertFalse((self.root / "build-called").exists())

    def test_unavailable_identity_fails_before_build(self):
        result = self.run_bundle("--identity", "Developer ID Application: Missing (NONE)")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "build-called").exists())

    def test_adhoc_bundle_has_version_and_a_valid_resource_seal(self):
        result = self.run_bundle("--ad-hoc")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        version = subprocess.check_output([
            "/usr/libexec/PlistBuddy", "-c", "Print :CFBundleShortVersionString",
            str(self.app / "Contents/Info.plist"),
        ], text=True).strip()
        self.assertEqual(version, "1.2.3")
        subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(self.app)], check=True)
        (self.app / "Contents/Resources/icon.icns").write_bytes(b"changed")
        check = subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(self.app)], capture_output=True)
        self.assertNotEqual(check.returncode, 0)

    def test_signing_failure_keeps_previous_bundle(self):
        self.app.mkdir(parents=True)
        (self.app / "keep").write_text("previous build")
        self.tool("codesign", "exit 1")
        result = self.run_bundle("--ad-hoc")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.app / "keep").read_text(), "previous build")

    def test_help_and_invalid_options_do_not_build(self):
        self.assertEqual(self.run_bundle("--help").returncode, 0)
        for args in (("--unknown",), ("--identity",), ("--identity", "-")):
            self.assertNotEqual(self.run_bundle(*args).returncode, 0)
        self.assertFalse((self.root / "build-called").exists())

    def notary_tools(self):
        # Apple authentication and notarization are external services. The
        # fixture uses their plist response format and real ZIP packaging.
        self.env["TEST_IDENTITIES"] = '  1) ' + 'A' * 40 + ' "Developer ID Application: Test (TEAM)"'
        self.tool("codesign", "exit 0")
        self.tool("spctl", 'exit "${GATEKEEPER_EXIT:-0}"')
        self.tool("xcrun", '''
case "$1 $2" in
  "notarytool history") exit "${HISTORY_EXIT:-0}" ;;
  "notarytool submit")
    cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>id</key><string>00000000-0000-0000-0000-000000000001</string>
<key>status</key><string>${NOTARY_STATUS:-Accepted}</string>
<key>message</key><string>Processing complete</string>
</dict></plist>
EOF
    exit "${SUBMIT_EXIT:-0}"
    ;;
  "notarytool log")
    for arg do last=$arg; done
    printf '%s\\n' '{"status":"Invalid","issues":[{"message":"test rejection"}]}' > "$last"
    ;;
  "stapler staple")
    [ "${STAPLE_EXIT:-0}" = 0 ] || exit 1
    touch "$3/test-ticket"
    ;;
  "stapler validate") test -f "$3/test-ticket" ;;
  *) exit 1 ;;
esac
''')

    def test_release_zip_contains_stapled_app(self):
        self.notary_tools()
        result = self.run_bundle("--notary-profile", "test profile")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        archives = list((self.root / "target/release").glob("Blink-*.zip"))
        self.assertEqual(len(archives), 1)
        with zipfile.ZipFile(archives[0]) as archive:
            self.assertIn("Blink.app/test-ticket", archive.namelist())
            self.assertIn("Blink.app/Contents/MacOS/blink", archive.namelist())

    def test_rejected_submission_keeps_previous_release_and_saves_log(self):
        self.notary_tools()
        self.env["NOTARY_STATUS"] = "Invalid"
        self.app.mkdir(parents=True)
        (self.app / "keep").write_text("previous build")
        archive = self.root / "target/release/Blink-1.2.3-macos-arm64.zip"
        archive.write_bytes(b"previous release")
        result = self.run_bundle()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.app / "keep").read_text(), "previous build")
        self.assertEqual(archive.read_bytes(), b"previous release")
        logs = list((self.root / "target/release").glob("notarization.*/log.json"))
        self.assertEqual(len(logs), 1)
        self.assertIn("test rejection", logs[0].read_text())

    def test_failed_release_checks_do_not_publish_an_app_or_zip(self):
        self.notary_tools()
        for flag in ("SUBMIT_EXIT", "STAPLE_EXIT", "GATEKEEPER_EXIT"):
            with self.subTest(flag=flag):
                self.env[flag] = "1"
                result = self.run_bundle()
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.app.exists())
                self.assertEqual(list((self.root / "target/release").glob("Blink-*.zip")), [])
                del self.env[flag]

    def test_missing_notary_credentials_fail_before_build(self):
        self.notary_tools()
        self.env["HISTORY_EXIT"] = "1"
        result = self.run_bundle()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "build-called").exists())

    def test_sign_only_does_not_require_notary_credentials_or_create_zip(self):
        self.notary_tools()
        self.env["HISTORY_EXIT"] = "1"
        result = self.run_bundle("--sign-only")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(self.app.exists())
        self.assertEqual(list((self.root / "target/release").glob("Blink-*.zip")), [])


if __name__ == "__main__":
    unittest.main()
