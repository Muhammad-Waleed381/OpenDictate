#!/usr/bin/env python3
"""Adds the Windows CUDA DLLs to the installer.

Runs in CI before `tauri build` on the Windows CUDA job. Reads the extracted
library directory from SHERPA_ONNX_LIB_DIR and adds each runtime DLL to
bundle.resources in src-tauri/tauri.conf.json. On Windows the resource
directory is the install directory, so the DLLs land next to the executable,
where Windows looks for them.

Map semantics for bundle.resources: key is the source on the build machine,
value is the destination inside the install directory.
"""
import glob
import json
import os
import sys

# Shipped with the archive but never loaded: TensorRT needs its own runtime,
# and the app only uses the C API.
SKIP = ("tensorrt", "cxx-api")

libdir = os.environ.get("SHERPA_ONNX_LIB_DIR")
if not libdir or not os.path.isdir(libdir):
    sys.exit("SHERPA_ONNX_LIB_DIR is not set or not a directory")

dlls = sorted(
    path
    for path in glob.glob(os.path.join(libdir, "*.dll"))
    if not any(skip in os.path.basename(path).lower() for skip in SKIP)
)
if not dlls:
    sys.exit(f"no .dll files found in {libdir}")

conf_path = "src-tauri/tauri.conf.json"
with open(conf_path) as f:
    conf = json.load(f)

resources = conf.setdefault("bundle", {}).setdefault("resources", {})
if isinstance(resources, list):
    sys.exit("bundle.resources is a list; expected a map")
resources.update({path: os.path.basename(path) for path in dlls})

with open(conf_path, "w") as f:
    json.dump(conf, f, indent=2)
    f.write("\n")

print(f"added {len(dlls)} DLLs to bundle.resources: {', '.join(map(os.path.basename, dlls))}")
