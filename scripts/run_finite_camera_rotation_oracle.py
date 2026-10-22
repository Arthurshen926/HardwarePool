#!/usr/bin/env python3
"""Mandatory disposable-hosted Mesa pixel gate for production finite-camera GLSL.

This runner exports inputs from the compiled production Java transform, invokes
only the explicitly provided native oracle, and rejects missing/duplicate cases,
partial output, absent negative controls, or a false success marker. No fallback,
skip, display server, camera, encoder, or physical GPU path exists.

The C oracle is separately compiled in CI with official Mesa EGL/GLES headers.
Pure parser tests may run locally; the Java-export/native-render path is hosted
only. This proves the host shader/transform algorithm with modeled AOSP producer
matrices, not Android SurfaceTexture metadata or physical camera behavior.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
EXPORT_MAIN = "io.capyio.camera.contract.FiniteCameraTransformContractTest"
HEADER = "# name facing sensor rotation producer geometry[16] texture[16] producer[16]"
PRODUCERS = ("raw", "inset1", "inset2")
EXPECTED_CASES = set(itertools.product(("back", "front"), (0, 90, 180, 270), PRODUCERS))
EXPECTED_NEGATIVES = {
    "wrong_rotation_sign", "unrequested_mirror", "stretched_portrait",
    "double_sensor_rotation", "transparent_black_bars",
}
PIXELS = 1280 * 720
MARKER = (
    "CAPYIO_FINITE_CAMERA_ROTATION_PIXELS_PASS cases=24 negative_controls=5 "
    "pixels=22118400 software=llvmpipe external_oes=true owned_cleanup=true"
)
EXPORT_FILES = ("vertex.glsl", "fragment.glsl", "vertices.txt", "cases.tsv")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def software_environment(environment: dict[str, str], opted_in: bool) -> dict[str, str]:
    """Do not let a local or self-hosted process accidentally execute GL work."""
    require(opted_in, "--hosted-software-only is required")
    require(environment.get("GITHUB_ACTIONS") == "true"
            and environment.get("RUNNER_ENVIRONMENT") == "github-hosted",
            "native execution requires the disposable GitHub-hosted runner")
    env = environment.copy()
    # Force the software platform. Do not inherit display/session or driver hooks.
    for name in tuple(env):
        if (name.startswith(("MESA_", "GALLIUM_", "LIBGL_", "__EGL_", "EGL_", "DRI_"))
                or name in {"DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "GBM_BACKEND",
                            "LD_PRELOAD", "LD_LIBRARY_PATH"}):
            env.pop(name)
    env.update({"CAPYIO_HOSTED_CAMERA_ROTATION": "1", "LIBGL_ALWAYS_SOFTWARE": "1",
                "GALLIUM_DRIVER": "llvmpipe", "EGL_PLATFORM": "surfaceless",
                "LP_NUM_THREADS": "1", "MESA_SHADER_CACHE_DISABLE": "true"})
    return env


def validate_exports(directory: Path) -> dict[str, str]:
    """Parse complete exported production data, retaining exact source hashes."""
    data = {}
    for name in EXPORT_FILES:
        path = directory / name
        require(path.is_file() and not path.is_symlink(), f"missing regular export: {name}")
        raw = path.read_bytes()
        require(0 < len(raw) <= 65536 and b"\0" not in raw, f"invalid export bounds: {name}")
        data[name] = raw.decode("utf-8")
    for stage in ("vertex.glsl", "fragment.glsl"):
        require("void main" in data[stage], f"missing production main: {stage}")
    require("samplerExternalOES" in data["fragment.glsl"], "external OES shader is mandatory")
    vertices = data["vertices.txt"].split()
    require(len(vertices) == 16 and all(math.isfinite(float(x)) for x in vertices),
            "exactly 16 finite production vertex components required")
    lines = data["cases.tsv"].splitlines()
    require(lines and lines[0] == HEADER, "unexpected export case schema")
    seen = set()
    names = set()
    for line in lines[1:]:
        row = line.split()
        require(len(row) == 53, "each exported case must contain metadata and 48 floats")
        name, facing, sensor_text, rotation_text, producer = row[:5]
        require(re.fullmatch(r"[A-Za-z0-9_-]{1,79}", name) is not None, "invalid case name")
        sensor, rotation = int(sensor_text), int(rotation_text)
        key = facing, sensor, producer
        require(key in EXPECTED_CASES and key not in seen and name not in names,
                "unknown or duplicate export case")
        require(rotation == sensor, "MirrorNONE producer rotation must equal sensor for both facings")
        require(all(math.isfinite(float(value)) for value in row[5:]), "nonfinite exported matrix")
        seen.add(key)
        names.add(name)
    require(seen == EXPECTED_CASES, "all 24 production export cases are mandatory")
    return {name: hashlib.sha256(data[name].encode("utf-8")).hexdigest() for name in EXPORT_FILES}


def number(record: dict, key: str, minimum: int = 0) -> int:
    value = record.get(key)
    require(type(value) is int and value >= minimum, f"invalid {key} count")
    return value


def validate_results(output: str) -> dict:
    """Success requires every actual pixel record, not just a grep-able marker."""
    lines = output.splitlines()
    require(lines and lines[-1] == MARKER and lines.count(MARKER) == 1,
            "missing unique final native success marker")
    records = [json.loads(line) for line in lines[:-1]]
    require(all(type(record) is dict for record in records), "non-object oracle record")
    cases, negatives, summary = [], [], []
    for record in records:
        kind = record.get("kind")
        require(kind in {"case", "negative", "summary"}, "unknown oracle record")
        {"case": cases, "negative": negatives, "summary": summary}[kind].append(record)
    require(len(cases) == 24 and len(negatives) == 5 and len(summary) == 1,
            "mandatory case/negative/summary counts differ")
    require(records == cases + negatives + summary, "unexpected oracle result ordering")
    seen = set()
    names = set()
    exact_total = interpolation_total = bar_total = 0
    for case in cases:
        key = case.get("facing"), case.get("sensor"), case.get("producer")
        require(key in EXPECTED_CASES and key not in seen, "unknown or duplicate pixel case")
        require(type(case.get("sensor")) is int and case.get("rotation") == case["sensor"],
                "unexpected output orientation")
        require(isinstance(case.get("name"), str) and case["name"] not in names,
                "missing or duplicate pixel case name")
        names.add(case["name"])
        seen.add(key)
        require(case.get("passed") is True and number(case, "pixels") == PIXELS,
                "mandatory pixel case failed/incomplete")
        exact = number(case, "exact_interior", 150001)
        interpolation = number(case, "interpolation", 1001)
        boundary = number(case, "boundary")
        bars = number(case, "black_bar")
        require(exact + interpolation + boundary + bars == PIXELS, "unchecked pixels in case")
        portrait = case["sensor"] in (90, 270)
        require(boundary == (1440 if portrait else 0), "unexpected fit edge allowance")
        require(bars == (874 * 720 if portrait else 0), "unexpected exact black bar region")
        require(all(number(case, key) == 0 for key in ("rgb_errors", "alpha_errors", "row_errors")),
                "pixel or opaque-alpha or exact fitted-width mismatch")
        exact_total += exact
        interpolation_total += interpolation
        bar_total += bars
    require(seen == EXPECTED_CASES, "missing pixel coverage")
    seen_negatives = set()
    for negative in negatives:
        name = negative.get("name")
        require(name in EXPECTED_NEGATIVES and name not in seen_negatives,
                "unknown or duplicate oracle negative control")
        seen_negatives.add(name)
        require(negative.get("rejected") is True, "oracle failed to reject mutation")
        require(sum(number(negative, key) for key in ("rgb_errors", "alpha_errors", "row_errors")) > 100,
                "vacuous oracle negative control")
    final = summary[0]
    require(final.get("passed") is True and final.get("opaque_alpha") is True
            and final.get("owned_cleanup") is True, "missing final proof/cleanup")
    require(number(final, "cases") == 24 and number(final, "negative_controls") == 5
            and number(final, "pixels") == PIXELS * 24, "wrong aggregate coverage")
    require(number(final, "exact_interior") == exact_total
            and number(final, "interpolation") == interpolation_total
            and number(final, "black_bar") == bar_total, "inconsistent aggregate pixel counts")
    return {"cases": cases, "negative_controls": negatives, "summary": final}


def run_logged(command: list[str], stdout: Path, stderr: Path, env: dict[str, str], timeout: int) -> str:
    with stdout.open("wb") as out, stderr.open("wb") as err:
        try:
            result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=out, stderr=err,
                                    env=env, timeout=timeout, check=False)
        except subprocess.TimeoutExpired as error:
            raise ValueError(f"bounded command timed out after {timeout}s: {command[0]}") from error
    require(result.returncode == 0,
            f"command failed ({result.returncode}): {command[0]}; see {stderr.name}")
    raw = stdout.read_bytes()
    require(len(raw) <= 1024 * 1024, "oversized command result")
    return raw.decode("utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hosted-software-only", action="store_true")
    parser.add_argument("--classes", type=Path, required=True, help="compiled production Java + contract-test classes")
    parser.add_argument("--oracle", type=Path, required=True, help="CI-built native EGL/GLES2 oracle")
    parser.add_argument("--artifacts", type=Path, required=True, help="fresh empty result directory")
    args = parser.parse_args()
    try:
        env = software_environment(dict(os.environ), args.hosted_software_only)
        require(args.classes.is_dir(), "compiled Java classes directory is missing")
        require(args.oracle.is_file() and os.access(args.oracle, os.X_OK), "built native oracle is missing")
        artifacts = args.artifacts.resolve()
        require(not artifacts.exists() or (artifacts.is_dir() and not any(artifacts.iterdir())),
                "artifact directory must be new or empty")
        artifacts.mkdir(parents=True, exist_ok=True)
        exported = artifacts / "production-export"
        exported.mkdir()
        run_logged(["java", "-ea", "-cp", str(args.classes.resolve()), EXPORT_MAIN, "--export", str(exported)],
                   artifacts / "java-export.stdout.txt", artifacts / "java-export.stderr.txt", env, 60)
        hashes = validate_exports(exported)
        source = ROOT / "scripts/native/finite_camera_rotation_oracle.c"
        manifest = {
            "schema": 1,
            "scope": "Hosted Mesa llvmpipe GLES2 shader/transform pixel algorithm only",
            "not_proven": ["Android SurfaceTexture producer metadata", "Camera2 or MediaCodec behavior",
                           "Android native/YUV EGLImage conversion", "physical hardware or performance"],
            "production_export_sha256": hashes,
            "native_source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "native_binary_sha256": hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
            "reference": "independent piecewise FlipV * Crop * Rotation(sensor), identical front/back MirrorNONE",
            "tolerance": {"flat_interior_rgb": 0, "alpha": 0, "bilinear_rgb": 3,
                          "fit_boundary_columns": [437, 842], "portrait_nonblack_pixels_per_row": 405,
                          "ideal_portrait_left": 437.5, "ideal_portrait_width": 405.0},
        }
        (artifacts / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        output = run_logged([str(args.oracle.resolve()), str(exported), str(artifacts)],
                            artifacts / "native.stdout.txt", artifacts / "native.stderr.txt", env, 120)
        result = validate_results(output)
        (artifacts / "results.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(MARKER)
        print("finite_camera_rotation_scope=host_gl_algorithm_only android_device_proof=false")
        return 0
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as error:
        print(f"finite_camera_rotation: FAIL {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
