"""
Build crates/extractors/archive/tests/fixtures/fixtures.zip

Outer archive: ZIP (enables streaming content extraction tests)

Inner archives (members of the ZIP):
  inner.tar      - plain TAR
  inner.tgz      - gzip-compressed TAR
  inner.tar.bz2  - bzip2-compressed TAR
  inner.tar.xz   - xz-compressed TAR
  inner.zip      - nested ZIP
  inner.7z       - 7-zip archive (built via 7z CLI)

Each inner archive contains the same set of test members:
  hello.txt              - simple text file
  subdir/greet.txt       - file in a subdirectory
  unicode/Ω.txt          - unicode filename
  deep/a/b/c/d/e/f.txt   - several levels of nesting
  long_<200 x's>.txt     - 200-char filename

The outer ZIP also includes the original fixtures.tgz so that tests for
deeply-nested paths, PAX headers, hardlinks etc. can continue to use that
fixture directly (it remains a member of the ZIP).
"""

import io
import os
import subprocess
import sys
import tarfile
import tempfile
import zipfile

FIXTURES_DIR = os.path.join(
    os.path.dirname(__file__),  # /tmp, but we'll use the explicit path below
)

OUT = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "../home/jamiet/code/find-anything/crates/extractors/archive/tests/fixtures/fixtures.zip",
)
# Override with absolute path
OUT = "/home/jamiet/code/find-anything/crates/extractors/archive/tests/fixtures/fixtures.zip"
ORIG_TGZ = "/home/jamiet/code/find-anything/crates/extractors/archive/tests/fixtures/fixtures.tgz"

LONG_NAME = "long_" + "x" * 195 + ".txt"  # 200 chars total

# Files to put inside every inner archive
INNER_MEMBERS = {
    "hello.txt": b"Hello from inner archive!\n",
    "subdir/greet.txt": b"Greetings from a subdirectory.\n",
    "unicode/\u03a9.txt": "Unicode omega: \u03a9\n".encode(),
    "deep/a/b/c/d/e/f.txt": b"deeply nested file\n",
    LONG_NAME: b"file with a very long name\n",
}


# ── helpers ──────────────────────────────────────────────────────────────────

def make_tar(members: dict, *, compression: str = "") -> bytes:
    """Return a TAR (optionally compressed) as bytes."""
    buf = io.BytesIO()
    mode = "w" + (f":{compression}" if compression else "")
    with tarfile.open(fileobj=buf, mode=mode) as tf:
        for name, data in members.items():
            info = tarfile.TarInfo(name=name)
            info.size = len(data)
            tf.addfile(info, io.BytesIO(data))
    return buf.getvalue()


def make_zip(members: dict) -> bytes:
    """Return a ZIP as bytes."""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for name, data in members.items():
            zf.writestr(name, data)
    return buf.getvalue()


def make_rar(members: dict) -> bytes:
    """Build a RAR archive via the CLI and return its bytes."""
    with tempfile.TemporaryDirectory() as td:
        for name, data in members.items():
            dest = os.path.join(td, name.replace("/", os.sep))
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            with open(dest, "wb") as f:
                f.write(data)

        archive = os.path.join(td, "_out.rar")
        # -ep1: store relative paths from the source base dir
        # -r:   recurse into subdirectories
        args = ["rar", "a", "-ep1", "-r", archive, td + "/"]
        subprocess.run(args, check=True, capture_output=True)

        with open(archive, "rb") as f:
            return f.read()


def make_7z(members: dict) -> bytes:
    """Build a 7z archive via the CLI and return its bytes."""
    with tempfile.TemporaryDirectory() as td:
        # Write member files to temp dir
        for name, data in members.items():
            dest = os.path.join(td, name.replace("/", os.sep))
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            with open(dest, "wb") as f:
                f.write(data)

        archive = os.path.join(td, "_out.7z")
        # Add all files; -spf keeps relative paths from the temp dir root
        args = ["7z", "a", "-spf2", archive]
        for name in members:
            args.append(os.path.join(td, name.replace("/", os.sep)))
        subprocess.run(args, check=True, capture_output=True)

        with open(archive, "rb") as f:
            return f.read()


# ── build ─────────────────────────────────────────────────────────────────────

inner_archives = {
    "inner.tar":     make_tar(INNER_MEMBERS),
    "inner.tgz":     make_tar(INNER_MEMBERS, compression="gz"),
    "inner.tar.bz2": make_tar(INNER_MEMBERS, compression="bz2"),
    "inner.tar.xz":  make_tar(INNER_MEMBERS, compression="xz"),
    "inner.zip":     make_zip(INNER_MEMBERS),
    "inner.7z":      make_7z(INNER_MEMBERS),
    "inner.rar":     make_rar(INNER_MEMBERS),
}

with zipfile.ZipFile(OUT, "w", compression=zipfile.ZIP_DEFLATED) as outer:
    # Add each inner archive
    for name, data in inner_archives.items():
        outer.writestr(name, data)
        print(f"  added {name} ({len(data):,} bytes)")

    # Include the original fixtures.tgz for its challenging TAR content
    with open(ORIG_TGZ, "rb") as f:
        outer.writestr("fixtures.tgz", f.read())
    print(f"  added fixtures.tgz")

print(f"\nWrote {OUT}")
print(f"Size: {os.path.getsize(OUT):,} bytes")

# Verify
with zipfile.ZipFile(OUT) as zf:
    print("\nOuter ZIP members:")
    for info in zf.infolist():
        print(f"  {info.filename:30s}  {info.file_size:>10,} bytes")
