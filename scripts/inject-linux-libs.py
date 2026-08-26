#!/usr/bin/env python3
"""Injects the Linux GPU shared libraries into tauri.conf.json.

Runs in CI before `tauri build` on the Linux GPU job. Reads the extracted
library directory from SHERPA_ONNX_LIB_DIR and fills bundle.linux.deb.files
and bundle.linux.rpm.files so the .deb/.rpm ship the .so files alongside a
binary whose RUNPATH (baked by src-tauri/build.rs from
OPENDICTATE_LINUX_RPATH) points at /usr/lib/opendictate.

Map semantics (verified empirically against tauri-cli 2.x): key is the
destination inside the package, value is the source on the build machine.
"""
import glob
import json
import os
import sys

DEST_DIR = "/usr/lib/opendictate"

libdir = os.environ.get("SHERPA_ONNX_LIB_DIR")
if not libdir or not os.path.isdir(libdir):
    sys.exit("SHERPA_ONNX_LIB_DIR is not set or not a directory")

sos = sorted(set(glob.glob(os.path.join(libdir, "*.so")) + glob.glob(os.path.join(libdir, "*.so.*"))))
if not sos:
    sys.exit(f"no .so files found in {libdir}")

# key = destination inside the package, value = source on the build machine
mapping = {f"{DEST_DIR}/{os.path.basename(so)}": so for so in sos}

conf_path = "src-tauri/tauri.conf.json"
with open(conf_path) as f:
    conf = json.load(f)

linux = conf.setdefault("bundle", {}).setdefault("linux", {})
linux["deb"] = {"files": mapping}
linux["rpm"] = {"files": mapping}
linux["appimage"] = {"files": mapping}

with open(conf_path, "w") as f:
    json.dump(conf, f, indent=2)
    f.write("\n")

print(f"injected {len(mapping)} shared libraries into deb/rpm/appimage files map")

