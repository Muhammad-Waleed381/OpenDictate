#!/usr/bin/env python3
"""Extract a zip/tar GPU library archive by content, regardless of its filename."""
from pathlib import Path
import sys
import tarfile
import zipfile


def inside(root, name):
    path = Path(name)
    destination = (root / path).resolve()
    if path.is_absolute() or not destination.is_relative_to(root):
        raise ValueError(f'archive path escapes destination: {name}')
    return destination


def extract(archive, destination):
    root = Path(destination).resolve()
    root.mkdir(parents=True, exist_ok=True)
    if zipfile.is_zipfile(archive):
        with zipfile.ZipFile(archive) as zipped:
            for entry in zipped.infolist():
                inside(root, entry.filename)
                zipped.extract(entry, root)
    else:
        with tarfile.open(archive, 'r:*') as tar:
            for entry in tar:
                path = inside(root, entry.name)
                if entry.issym():
                    inside(root, str(path.parent.relative_to(root) / entry.linkname))
                elif entry.islnk():
                    inside(root, entry.linkname)
                elif not (entry.isfile() or entry.isdir()):
                    raise ValueError(f'unsupported archive entry: {entry.name}')
                # Validate immediately before extracting: an earlier symlink
                # must not redirect a later member outside the destination.
                # The 'data' filter is requested explicitly: on Python < 3.12
                # the default is 'fully_trusted', which would allow a member
                # to be written through a symlink created by an earlier one.
                tar.extract(entry, root, filter='data')


if __name__ == '__main__':
    if len(sys.argv) != 3:
        sys.exit('usage: extract-gpu-libs.py ARCHIVE DESTINATION')
    extract(sys.argv[1], sys.argv[2])
