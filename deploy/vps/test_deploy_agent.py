import hashlib
import importlib.util
import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

AGENT_PATH = Path(__file__).resolve().parent / "qrcardapio-deploy-agent.py"
SPEC = importlib.util.spec_from_file_location("qrcardapio_deploy_agent", AGENT_PATH)
assert SPEC and SPEC.loader
agent = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = agent
SPEC.loader.exec_module(agent)


def make_bundle(dest: Path, extra_members: list[tuple[str, bytes, str]] | None = None) -> str:
    with tarfile.open(dest, "w:gz") as tar:
        for name, data in [
            ("bin/qrcardapio", b"#!/bin/sh\necho ok\n"),
            ("static/css/tokens.css", b":root{}\n"),
            ("release.json", b'{"sha":"ok"}\n'),
        ]:
            info = tarfile.TarInfo(name=name)
            info.size = len(data)
            info.mode = 0o755 if name.startswith("bin/") else 0o644
            tar.addfile(info, io.BytesIO(data))
        for name, data, kind in extra_members or []:
            info = tarfile.TarInfo(name=name)
            if kind == "symlink":
                info.type = tarfile.SYMTYPE
                info.linkname = "/etc/passwd"
                tar.addfile(info)
            else:
                info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
    return hashlib.sha256(dest.read_bytes()).hexdigest()


class FakeOps(agent.SystemOps):
    def __init__(
        self,
        *,
        manifest: dict,
        bundle_src: Path,
        attest_src: Path,
        main_head: str,
        fail_smoke_on_first_restart: bool = False,
        etag: str = '"etag-1"',
    ) -> None:
        self.manifest = manifest
        self.bundle_src = bundle_src
        self.attest_src = attest_src
        self.main_head = main_head
        self.fail_smoke_on_first_restart = fail_smoke_on_first_restart
        self.etag = etag
        self.restarts: list[str] = []
        self.cli_calls: list[list[str]] = []
        self.attested_shas: list[str] = []
        self.release_fetches = 0

    def fetch_json(self, url: str, etag: str | None = None):
        if "/releases/tags/" in url:
            self.release_fetches += 1
            if etag == self.etag:
                return 304, None, self.etag
            return (
                200,
                {
                    "assets": [
                        {
                            "name": "manifest.json",
                            "browser_download_url": "https://example.test/manifest.json",
                        },
                        {
                            "name": self.manifest["bundle"],
                            "browser_download_url": f"https://example.test/{self.manifest['bundle']}",
                        },
                        {
                            "name": self.manifest["attestation_bundle"],
                            "browser_download_url": f"https://example.test/{self.manifest['attestation_bundle']}",
                        },
                    ]
                },
                self.etag,
            )
        if "/git/ref/heads/main" in url:
            return 200, {"object": {"sha": self.main_head}}, None
        raise AssertionError(f"unexpected URL {url}")

    def download_file(self, url: str, dest: Path, max_bytes: int) -> None:
        if url.endswith("manifest.json"):
            dest.write_text(json.dumps(self.manifest), encoding="utf-8")
        elif url.endswith(self.manifest["bundle"]):
            dest.write_bytes(self.bundle_src.read_bytes())
        elif url.endswith(self.manifest["attestation_bundle"]):
            dest.write_bytes(self.attest_src.read_bytes())
        else:
            raise AssertionError(f"unexpected download {url}")

    def verify_attestation(self, bundle, sigstore_bundle, cfg, commit_sha):
        self.attested_shas.append(commit_sha)

    def sanity_check_binary(self, binary: Path) -> None:
        assert binary.is_file()

    def run_as_app_user(self, cfg, binary: Path, args: list[str]) -> None:
        self.cli_calls.append(args)
        if args[:2] == ["db", "backup"]:
            Path(args[3]).write_bytes(b"sqlite-backup")

    def restart_service(self, service_name: str) -> None:
        self.restarts.append(service_name)

    def http_probe(self, url: str):
        path = "/" + url.split("://", 1)[1].split("/", 1)[1]
        if self.fail_smoke_on_first_restart and len(self.restarts) == 1 and path == "/sitemap.xml":
            return 500, {}, "broken"
        if path == "/healthz":
            return 200, {}, '{"status":"ok"}'
        if path == "/readyz":
            return 200, {}, '{"status":"ready"}'
        if path == "/mcp":
            return (
                401,
                {
                    "www-authenticate": 'Bearer resource_metadata="https://qrcardapio.com.br/.well-known/oauth-protected-resource/mcp"'
                },
                "{}",
            )
        return 200, {}, "ok"


class DeployAgentTests(unittest.TestCase):
    def test_safe_extract_rejects_traversal_and_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bad_trav = root / "trav.tar.gz"
            make_bundle(bad_trav, [("../escape.txt", b"x", "file")])
            with self.assertRaises(agent.DeployError):
                agent.safe_extract_bundle(bad_trav, root / "out1")

            bad_sym = root / "sym.tar.gz"
            make_bundle(bad_sym, [("static/link", b"", "symlink")])
            with self.assertRaises(agent.DeployError):
                agent.safe_extract_bundle(bad_sym, root / "out2")

    def test_validate_manifest_rejects_invalid_fields(self):
        valid = {
            "repo": "AlexandreZanata/QRcardapio",
            "sha": "a" * 40,
            "short_sha": "a" * 12,
            "bundle": "qrcardapio-aaaaaaaaaaaa-linux-amd64.tar.gz",
            "bundle_sha256": "b" * 64,
            "attestation_bundle": "qrcardapio-aaaaaaaaaaaa-linux-amd64.tar.gz.sigstore.json",
            "ref": "refs/heads/main",
        }
        self.assertEqual(
            agent.validate_manifest(valid, "AlexandreZanata/QRcardapio")["sha"],
            "a" * 40,
        )
        bad_repo = {**valid, "repo": "evil/fork"}
        with self.assertRaises(agent.DeployError):
            agent.validate_manifest(bad_repo, "AlexandreZanata/QRcardapio")
        bad_ref = {**valid, "ref": "refs/heads/pr-1"}
        with self.assertRaises(agent.DeployError):
            agent.validate_manifest(bad_ref, "AlexandreZanata/QRcardapio")

    def test_full_deploy_backup_etag_noop_and_auto_rollback(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cfg = agent.Config(
                opt_dir=root / "opt",
                data_dir=root / "data",
                state_dir=root / "state",
                backups_dir=root / "backups",
                env_file=root / "server.env",
                health_timeout_secs=3,
            )
            (cfg.data_dir / "db").mkdir(parents=True)
            (cfg.data_dir / "db" / "qrcardapio.sqlite").write_bytes(b"db")
            cfg.env_file.write_text("MCP_ENABLED=true\n", encoding="utf-8")

            sha1 = "1" * 40
            bundle1 = root / "b1.tar.gz"
            digest1 = make_bundle(bundle1)
            attest1 = root / "b1.sigstore.json"
            attest1.write_text("{}", encoding="utf-8")
            manifest1 = {
                "repo": cfg.repo,
                "sha": sha1,
                "short_sha": sha1[:12],
                "bundle": f"qrcardapio-{sha1[:12]}-linux-amd64.tar.gz",
                "bundle_sha256": digest1,
                "attestation_bundle": f"qrcardapio-{sha1[:12]}-linux-amd64.tar.gz.sigstore.json",
                "ref": "refs/heads/main",
            }
            ops1 = FakeOps(
                manifest=manifest1, bundle_src=bundle1, attest_src=attest1, main_head=sha1
            )
            self.assertEqual(agent.deploy_once(cfg, ops1), "deployed")
            self.assertEqual(ops1.attested_shas, [sha1])
            self.assertEqual(cfg.current_link.resolve(), (cfg.releases_dir / sha1[:12]).resolve())
            self.assertEqual(len(list(cfg.backups_dir.glob("pre-*.sqlite"))), 1)
            self.assertTrue(any(call == ["republish-all"] for call in ops1.cli_calls))

            # Second poll with identical ETag must short-circuit with 304 noop.
            self.assertEqual(agent.deploy_once(cfg, ops1), "noop")

            # Broken release sha2 must fail smoke check, auto-rollback to sha1, and record failed_sha.
            sha2 = "2" * 40
            bundle2 = root / "b2.tar.gz"
            digest2 = make_bundle(bundle2)
            manifest2 = {
                "repo": cfg.repo,
                "sha": sha2,
                "short_sha": sha2[:12],
                "bundle": f"qrcardapio-{sha2[:12]}-linux-amd64.tar.gz",
                "bundle_sha256": digest2,
                "attestation_bundle": f"qrcardapio-{sha2[:12]}-linux-amd64.tar.gz.sigstore.json",
                "ref": "refs/heads/main",
            }
            ops2 = FakeOps(
                manifest=manifest2,
                bundle_src=bundle2,
                attest_src=attest1,
                main_head=sha2,
                fail_smoke_on_first_restart=True,
                etag='"etag-2"',
            )
            with self.assertRaises(agent.DeployError):
                agent.deploy_once(cfg, ops2)
            self.assertEqual(cfg.current_link.resolve(), (cfg.releases_dir / sha1[:12]).resolve())
            self.assertEqual(len(ops2.restarts), 2)
            state = agent.load_state(cfg.state_file)
            self.assertEqual(state["current_sha"], sha1)
            self.assertEqual(state["failed_sha"], sha2)

            # Subsequent poll with new ETag for same failed SHA skips cleanly instead of crash-looping.
            ops2.etag = '"etag-2-again"'
            self.assertEqual(agent.deploy_once(cfg, ops2), "skipped_failed")


if __name__ == "__main__":
    unittest.main()
