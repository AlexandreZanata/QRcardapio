#!/usr/bin/env python3
"""Pull-based, Sigstore-attested auto-deploy agent for QR Cardapio on Linux VPS.

Polls the public GitHub Release `deploy-main` using ETag conditional requests (no GitHub
token or SSH key required), verifies that the manifest targets the current `main` HEAD,
checks SHA-256 and offline Sigstore build provenance (`gh attestation verify --bundle`),
takes an online SQLite backup (`VACUUM INTO`), atomically swaps `/opt/qrcardapio/current`,
restarts only `qrcardapio.service`, validates health/SEO/MCP smoke checks, republishes
static menus, and automatically rolls back on any post-cutover failure.
"""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

USER_AGENT = "qrcardapio-deploy-agent/1.0"
HEX40_RE = re.compile(r"^[0-9a-f]{40}$")
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")
SAFE_ASSET_RE = re.compile(r"^[A-Za-z0-9._-]{1,128}$")
MAX_BUNDLE_BYTES = 128 * 1024 * 1024
MAX_META_BYTES = 2 * 1024 * 1024
KEEP_RELEASES = 5
KEEP_BACKUPS = 10


class DeployError(RuntimeError):
    """Raised when candidate verification or activation fails."""


class TransientError(RuntimeError):
    """Raised for temporary network/API errors that should retry on the next tick."""


def log(event: str, **fields: object) -> None:
    payload = {
        "ts": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "event": event,
        **fields,
    }
    print(json.dumps(payload, ensure_ascii=False), flush=True)


@dataclass(frozen=True)
class Config:
    repo: str = "AlexandreZanata/QRcardapio"
    release_tag: str = "deploy-main"
    workflow: str = "AlexandreZanata/QRcardapio/.github/workflows/deploy-bundle.yml"
    opt_dir: Path = Path("/opt/qrcardapio")
    data_dir: Path = Path("/var/lib/qrcardapio")
    state_dir: Path = Path("/var/lib/qrcardapio-deploy")
    backups_dir: Path = Path("/var/backups/qrcardapio")
    env_file: Path = Path("/etc/qrcardapio/server.env")
    service_name: str = "qrcardapio.service"
    app_user: str = "qrcardapio"
    health_base_url: str = "http://127.0.0.1:8090"
    health_timeout_secs: int = 20

    @property
    def releases_dir(self) -> Path:
        return self.opt_dir / "releases"

    @property
    def current_link(self) -> Path:
        return self.opt_dir / "current"

    @property
    def state_file(self) -> Path:
        return self.state_dir / "state.json"

    @property
    def lock_file(self) -> Path:
        return self.state_dir / "agent.lock"

    @classmethod
    def from_env(cls) -> "Config":
        repo = os.environ.get("QRCARDAPIO_REPO", cls.repo)
        return cls(
            repo=repo,
            release_tag=os.environ.get("QRCARDAPIO_RELEASE_TAG", cls.release_tag),
            workflow=os.environ.get(
                "QRCARDAPIO_WORKFLOW",
                f"{repo}/.github/workflows/deploy-bundle.yml",
            ),
            opt_dir=Path(os.environ.get("QRCARDAPIO_OPT_DIR", str(cls.opt_dir))),
            data_dir=Path(os.environ.get("QRCARDAPIO_DATA_DIR", str(cls.data_dir))),
            state_dir=Path(os.environ.get("QRCARDAPIO_STATE_DIR", str(cls.state_dir))),
            backups_dir=Path(os.environ.get("QRCARDAPIO_BACKUPS_DIR", str(cls.backups_dir))),
            env_file=Path(os.environ.get("QRCARDAPIO_ENV_FILE", str(cls.env_file))),
            service_name=os.environ.get("QRCARDAPIO_SERVICE", cls.service_name),
            app_user=os.environ.get("QRCARDAPIO_APP_USER", cls.app_user),
            health_base_url=os.environ.get(
                "QRCARDAPIO_HEALTH_URL", cls.health_base_url
            ).rstrip("/"),
            health_timeout_secs=int(
                os.environ.get("QRCARDAPIO_HEALTH_TIMEOUT", str(cls.health_timeout_secs))
            ),
        )


def parse_env_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.exists():
        return values
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, val = line.split("=", 1)
        val = val.strip()
        if len(val) >= 2 and val[0] == val[-1] and val[0] in ("'", '"'):
            val = val[1:-1]
        values[key.strip()] = val
    return values


def load_state(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        return data if isinstance(data, dict) else {}
    except (OSError, json.JSONDecodeError):
        return {}


def save_state(path: Path, state: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.chmod(tmp, 0o600)
    os.replace(tmp, path)


def validate_manifest(data: Any, expected_repo: str) -> dict[str, str]:
    if not isinstance(data, dict):
        raise DeployError("manifest.json must be a JSON object")
    required = ("repo", "sha", "short_sha", "bundle", "bundle_sha256", "attestation_bundle")
    out: dict[str, str] = {}
    for key in required:
        val = data.get(key)
        if not isinstance(val, str) or not val:
            raise DeployError(f"manifest missing required string field '{key}'")
        out[key] = val
    if out["repo"] != expected_repo:
        raise DeployError(f"manifest repo mismatch: {out['repo']}")
    if not HEX40_RE.match(out["sha"]):
        raise DeployError("manifest sha is not a 40-char lowercase hex commit hash")
    if out["short_sha"] != out["sha"][:12]:
        raise DeployError("manifest short_sha does not match sha prefix")
    if not HEX64_RE.match(out["bundle_sha256"]):
        raise DeployError("manifest bundle_sha256 is not a valid SHA-256 hex digest")
    for asset_key in ("bundle", "attestation_bundle"):
        if not SAFE_ASSET_RE.match(out[asset_key]) or ".." in out[asset_key]:
            raise DeployError(f"manifest asset name '{asset_key}' is unsafe")
    if data.get("ref") not in (None, "refs/heads/main"):
        raise DeployError(f"manifest ref must be refs/heads/main, got {data.get('ref')}")
    return out


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_extract_bundle(archive_path: Path, dest_dir: Path) -> None:
    if dest_dir.exists():
        shutil.rmtree(dest_dir)
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest_resolved = dest_dir.resolve()

    with tarfile.open(archive_path, "r:gz") as tar:
        members = tar.getmembers()
        if not members:
            raise DeployError("bundle archive is empty")
        for member in members:
            if member.issym() or member.islnk() or member.isdev() or member.isfifo():
                raise DeployError(f"forbidden special tar entry: {member.name}")
            if not (member.isfile() or member.isdir()):
                raise DeployError(f"unsupported tar entry type: {member.name}")
            if member.name.startswith("/") or "\\" in member.name:
                raise DeployError(f"absolute or invalid tar path: {member.name}")
            parts = Path(member.name).parts
            if not parts or any(part in ("..", "") for part in parts):
                raise DeployError(f"path traversal in tar entry: {member.name}")
            if parts[0] not in ("bin", "static", "release.json"):
                raise DeployError(f"unexpected top-level tar entry: {member.name}")
            target = (dest_dir / member.name).resolve()
            if target != dest_resolved and dest_resolved not in target.parents:
                raise DeployError(f"tar entry escapes destination: {member.name}")

            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                os.chmod(target, 0o755)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                src = tar.extractfile(member)
                if src is None:
                    raise DeployError(f"could not read tar entry: {member.name}")
                with src, target.open("wb") as out_fh:
                    shutil.copyfileobj(src, out_fh)
                os.chmod(target, 0o755 if parts[0] == "bin" else 0o644)

    binary = dest_dir / "bin" / "qrcardapio"
    static_dir = dest_dir / "static"
    if not binary.is_file() or not static_dir.is_dir():
        raise DeployError("bundle missing required bin/qrcardapio or static/")


def atomic_symlink(link_path: Path, target: Path) -> None:
    link_path.parent.mkdir(parents=True, exist_ok=True)
    tmp_link = link_path.parent / f".{link_path.name}.tmp.{os.getpid()}"
    if tmp_link.exists() or tmp_link.is_symlink():
        tmp_link.unlink()
    os.symlink(str(target), str(tmp_link))
    os.replace(tmp_link, link_path)


def prune_old_entries(directory: Path, keep_names: set[str], max_extra: int) -> None:
    if not directory.exists():
        return
    candidates = [
        entry
        for entry in directory.iterdir()
        if entry.name not in keep_names and not entry.name.startswith(".")
    ]
    candidates.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    for stale in candidates[max_extra:]:
        if stale.is_dir() and not stale.is_symlink():
            shutil.rmtree(stale, ignore_errors=True)
        else:
            stale.unlink(missing_ok=True)


class SystemOps:
    """Thin abstraction over HTTP, GitHub attestation, systemd, and app CLI calls."""

    def fetch_json(
        self, url: str, etag: str | None = None
    ) -> tuple[int, dict[str, Any] | None, str | None]:
        headers = {
            "User-Agent": USER_AGENT,
            "Accept": "application/vnd.github+json",
        }
        if etag:
            headers["If-None-Match"] = etag
        req = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=20) as resp:
                raw = resp.read(MAX_META_BYTES + 1)
                if len(raw) > MAX_META_BYTES:
                    raise DeployError(f"JSON response exceeded size limit: {url}")
                return resp.status, json.loads(raw.decode("utf-8")), resp.headers.get("ETag")
        except urllib.error.HTTPError as err:
            if err.code == 304:
                return 304, None, etag
            if err.code in (404, 429, 500, 502, 503, 504):
                raise TransientError(f"HTTP {err.code} fetching {url}") from err
            raise DeployError(f"HTTP {err.code} fetching {url}") from err
        except (urllib.error.URLError, TimeoutError, OSError) as err:
            raise TransientError(f"network error fetching {url}: {err}") from err

    def download_file(self, url: str, dest: Path, max_bytes: int) -> None:
        req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        try:
            with urllib.request.urlopen(req, timeout=60) as resp, dest.open("wb") as fh:
                total = 0
                while True:
                    chunk = resp.read(65536)
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > max_bytes:
                        raise DeployError(f"download exceeded {max_bytes} bytes: {url}")
                    fh.write(chunk)
        except urllib.error.HTTPError as err:
            if err.code in (404, 429, 500, 502, 503, 504):
                raise TransientError(f"HTTP {err.code} downloading {url}") from err
            raise DeployError(f"HTTP {err.code} downloading {url}") from err
        except (urllib.error.URLError, TimeoutError, OSError) as err:
            raise TransientError(f"network error downloading {url}: {err}") from err

    def verify_attestation(
        self, bundle: Path, sigstore_bundle: Path, cfg: Config, commit_sha: str
    ) -> None:
        env = os.environ.copy()
        env["GH_CONFIG_DIR"] = str(cfg.state_dir / "gh")
        env["XDG_CACHE_HOME"] = str(cfg.state_dir / "cache")
        env["XDG_DATA_HOME"] = str(cfg.state_dir / "data")
        cmd = [
            "gh",
            "attestation",
            "verify",
            str(bundle),
            "--bundle",
            str(sigstore_bundle),
            "--repo",
            cfg.repo,
            "--signer-workflow",
            cfg.workflow,
            "--source-ref",
            "refs/heads/main",
            "--source-digest",
            commit_sha,
            "--deny-self-hosted-runners",
        ]
        res = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=60)
        if res.returncode != 0:
            raise DeployError(f"Sigstore attestation verification failed: {res.stderr.strip()}")

    def sanity_check_binary(self, binary: Path) -> None:
        res = subprocess.run([str(binary), "--help"], capture_output=True, text=True, timeout=15)
        if res.returncode != 0:
            raise DeployError(f"binary sanity check failed: {res.stderr.strip()}")

    def run_as_app_user(self, cfg: Config, binary: Path, args: list[str]) -> None:
        app_env = os.environ.copy()
        app_env.update(parse_env_file(cfg.env_file))
        cmd = ["runuser", "-u", cfg.app_user, "--", str(binary), *args]
        res = subprocess.run(cmd, env=app_env, capture_output=True, text=True, timeout=60)
        if res.returncode != 0:
            raise DeployError(f"app CLI {' '.join(args)} failed: {res.stderr.strip()}")

    def restart_service(self, service_name: str) -> None:
        res = subprocess.run(
            ["systemctl", "restart", service_name],
            capture_output=True,
            text=True,
            timeout=60,
        )
        if res.returncode != 0:
            raise DeployError(f"systemctl restart {service_name} failed: {res.stderr.strip()}")

    def http_probe(self, url: str) -> tuple[int, dict[str, str], str]:
        req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        try:
            with urllib.request.urlopen(req, timeout=5) as resp:
                hdrs = {k.lower(): v for k, v in resp.headers.items()}
                return resp.status, hdrs, resp.read(65536).decode("utf-8", errors="replace")
        except urllib.error.HTTPError as err:
            hdrs = {k.lower(): v for k, v in err.headers.items()}
            body = err.read(65536).decode("utf-8", errors="replace")
            return err.code, hdrs, body
        except (urllib.error.URLError, TimeoutError, OSError) as err:
            raise DeployError(f"probe failed for {url}: {err}") from err


def wait_for_health_and_smoke(ops: SystemOps, cfg: Config, mcp_enabled: bool) -> None:
    deadline = time.monotonic() + cfg.health_timeout_secs
    last_err = "service not ready"
    while time.monotonic() < deadline:
        try:
            h_code, _, h_body = ops.http_probe(f"{cfg.health_base_url}/healthz")
            r_code, _, r_body = ops.http_probe(f"{cfg.health_base_url}/readyz")
            if h_code == 200 and '"ok"' in h_body and r_code == 200 and '"ready"' in r_body:
                break
            last_err = f"healthz={h_code} readyz={r_code}"
        except DeployError as err:
            last_err = str(err)
        time.sleep(0.5)
    else:
        raise DeployError(f"service failed readiness check: {last_err}")

    for path in ("/", "/en", "/es", "/robots.txt", "/sitemap.xml", "/llms.txt"):
        code, _, _ = ops.http_probe(f"{cfg.health_base_url}{path}")
        if code != 200:
            raise DeployError(f"smoke check failed: {path} returned HTTP {code}")

    if mcp_enabled:
        for path in (
            "/.well-known/oauth-protected-resource/mcp",
            "/.well-known/oauth-authorization-server",
        ):
            code, _, _ = ops.http_probe(f"{cfg.health_base_url}{path}")
            if code != 200:
                raise DeployError(f"MCP metadata check failed: {path} returned HTTP {code}")
        code, hdrs, _ = ops.http_probe(f"{cfg.health_base_url}/mcp")
        www_auth = hdrs.get("www-authenticate", "")
        if code != 401 or "oauth-protected-resource/mcp" not in www_auth:
            raise DeployError(f"MCP challenge check failed: HTTP {code} header={www_auth!r}")


def deploy_once(cfg: Config, ops: SystemOps, *, force: bool = False) -> str:
    cfg.state_dir.mkdir(parents=True, exist_ok=True)
    state = load_state(cfg.state_dir / "state.json")
    release_url = (
        f"https://api.github.com/repos/{cfg.repo}/releases/tags/{cfg.release_tag}"
    )
    etag = None if force else state.get("release_etag")
    status, release, new_etag = ops.fetch_json(release_url, etag=etag)
    if status == 304:
        log("noop_etag_unchanged", current_sha=state.get("current_sha"))
        return "noop"
    if not isinstance(release, dict):
        raise DeployError("unexpected GitHub release payload")

    assets = {
        a["name"]: a["browser_download_url"]
        for a in release.get("assets", [])
        if isinstance(a, dict) and "name" in a and "browser_download_url" in a
    }
    if "manifest.json" not in assets:
        raise TransientError("release missing manifest.json (upload may still be in progress)")

    with tempfile.TemporaryDirectory(prefix="qrcardapio-deploy-") as tmp_raw:
        tmp_dir = Path(tmp_raw)
        manifest_path = tmp_dir / "manifest.json"
        ops.download_file(assets["manifest.json"], manifest_path, MAX_META_BYTES)
        manifest = validate_manifest(
            json.loads(manifest_path.read_text(encoding="utf-8")), cfg.repo
        )
        sha = manifest["sha"]
        short_sha = manifest["short_sha"]

        if not force and state.get("current_sha") == sha:
            state["release_etag"] = new_etag
            save_state(cfg.state_file, state)
            log("noop_already_deployed", sha=sha)
            return "noop"
        if not force and state.get("failed_sha") == sha:
            state["release_etag"] = new_etag
            save_state(cfg.state_file, state)
            log("skip_failed_sha", sha=sha)
            return "skipped_failed"

        # Verify manifest points to current `main` HEAD (prevents stale/downgrade replay).
        ref_url = f"https://api.github.com/repos/{cfg.repo}/git/ref/heads/main"
        _, ref_data, _ = ops.fetch_json(ref_url)
        head_sha = (ref_data or {}).get("object", {}).get("sha")
        if head_sha != sha:
            log("defer_manifest_not_main_head", manifest_sha=sha, main_head=head_sha)
            return "deferred"

        bundle_name = manifest["bundle"]
        attest_name = manifest["attestation_bundle"]
        if bundle_name not in assets or attest_name not in assets:
            raise TransientError("release assets incomplete; will retry on next poll")

        bundle_path = tmp_dir / bundle_name
        attest_path = tmp_dir / attest_name
        ops.download_file(assets[bundle_name], bundle_path, MAX_BUNDLE_BYTES)
        ops.download_file(assets[attest_name], attest_path, MAX_META_BYTES)

        actual_sha256 = sha256_file(bundle_path)
        if actual_sha256 != manifest["bundle_sha256"]:
            raise DeployError(
                f"bundle SHA-256 mismatch: expected {manifest['bundle_sha256']}, got {actual_sha256}"
            )

        ops.verify_attestation(bundle_path, attest_path, cfg, sha)
        log("verified_candidate", sha=sha, bundle=bundle_name)

        release_dir = cfg.releases_dir / short_sha
        safe_extract_bundle(bundle_path, release_dir)
        new_bin = release_dir / "bin" / "qrcardapio"
        ops.sanity_check_binary(new_bin)

    # Consistent online SQLite backup before touching the live service.
    if (cfg.data_dir / "db" / "qrcardapio.sqlite").exists():
        cfg.backups_dir.mkdir(parents=True, exist_ok=True)
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        backup_out = cfg.backups_dir / f"pre-{short_sha}-{stamp}.sqlite"
        backup_bin = (
            cfg.current_link / "bin" / "qrcardapio"
            if (cfg.current_link / "bin" / "qrcardapio").is_file()
            else new_bin
        )
        ops.run_as_app_user(cfg, backup_bin, ["db", "backup", "--output", str(backup_out)])
        prune_old_entries(cfg.backups_dir, {backup_out.name}, KEEP_BACKUPS - 1)
        log("backup_created", path=str(backup_out))

    prev_target = cfg.current_link.resolve() if cfg.current_link.is_symlink() else None
    env_values = parse_env_file(cfg.env_file)
    mcp_enabled = env_values.get("MCP_ENABLED", "").strip().lower() in ("1", "true", "yes")

    try:
        atomic_symlink(cfg.current_link, release_dir)
        ops.restart_service(cfg.service_name)
        wait_for_health_and_smoke(ops, cfg, mcp_enabled=mcp_enabled)
        ops.run_as_app_user(cfg, new_bin, ["republish-all"])
    except Exception as exc:
        log("deploy_failed_rolling_back", sha=sha, error=str(exc))
        state["failed_sha"] = sha
        state["last_error"] = str(exc)
        state["release_etag"] = new_etag
        save_state(cfg.state_file, state)
        if prev_target and prev_target.exists():
            atomic_symlink(cfg.current_link, prev_target)
            ops.restart_service(cfg.service_name)
            wait_for_health_and_smoke(ops, cfg, mcp_enabled=mcp_enabled)
            log("rollback_succeeded", restored=str(prev_target))
        raise

    state["current_sha"] = sha
    state["current_release"] = str(release_dir)
    state["deployed_at"] = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    state["release_etag"] = new_etag
    state.pop("failed_sha", None)
    state.pop("last_error", None)
    save_state(cfg.state_file, state)

    keep = {release_dir.name}
    if prev_target and prev_target.parent == cfg.releases_dir:
        keep.add(prev_target.name)
    prune_old_entries(cfg.releases_dir, keep, max(0, KEEP_RELEASES - len(keep)))
    log("deploy_succeeded", sha=sha, release=str(release_dir))
    return "deployed"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="QR Cardapio pull-based auto-deploy agent")
    parser.add_argument("--force", action="store_true", help="Ignore cached ETag and SHA state")
    args = parser.parse_args(argv)

    cfg = Config.from_env()
    cfg.state_dir.mkdir(parents=True, exist_ok=True)
    with cfg.lock_file.open("w", encoding="utf-8") as lock_fh:
        try:
            fcntl.flock(lock_fh.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            log("skip_locked")
            return 0
        try:
            deploy_once(cfg, SystemOps(), force=args.force)
            return 0
        except TransientError as err:
            log("transient_error", error=str(err))
            return 0
        except Exception as err:
            log("fatal_deploy_error", error=str(err))
            return 1


if __name__ == "__main__":
    sys.exit(main())
