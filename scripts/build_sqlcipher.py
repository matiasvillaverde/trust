#!/usr/bin/env python3
"""Build the pinned stable SQLCipher backend locally, without root or global installs.

Requires Python 3.12+, a C toolchain, make, pkg-config and OpenSSL development files.
The service uses this native library rather than libsqlite3-sys's older bundled C.
"""
import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

VERSION = "4.19.0"
ARCHIVE_SHA256 = "7075f96cbabe45b4ecfc2e6b1745a625f856f695b0827a5506ce9ed85b906aa0"
URL = f"https://codeload.github.com/sqlcipher/sqlcipher/tar.gz/refs/tags/v{VERSION}"
ROOT = Path(__file__).resolve().parents[1]
PREFIX = ROOT / ".local" / "sqlcipher"


def main():
    if sys.platform != "linux":
        raise SystemExit("The production service native build currently targets Linux; use the Docker image on other hosts.")
    PREFIX.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(URL, timeout=60) as response:
        archive = response.read(32 * 1024 * 1024 + 1)
    if hashlib.sha256(archive).hexdigest() != ARCHIVE_SHA256:
        raise SystemExit("SQLCipher source checksum mismatch; refusing to build")
    with tempfile.TemporaryDirectory(prefix="sqlcipher-build-", dir=PREFIX.parent) as folder:
        folder = Path(folder)
        with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as bundle:
            bundle.extractall(folder, filter="data")
        source = folder / f"sqlcipher-{VERSION}"
        cflags = "-O2 -fPIC -fstack-protector-strong -D_FORTIFY_SOURCE=2 -DSQLITE_HAS_CODEC -DSQLITE_EXTRA_INIT=sqlcipher_extra_init -DSQLITE_EXTRA_SHUTDOWN=sqlcipher_extra_shutdown -DSQLITE_DEFAULT_MEMSTATUS=0 -DSQLITE_USE_URI=1"
        subprocess.run([
            str(source / "configure"), "--disable-shared", "--disable-tcl",
            "--disable-readline", "--disable-load-extension", "--with-tempstore=always",
            f"CFLAGS={cflags}", "LDFLAGS=-lcrypto",
        ], cwd=source, check=True)
        subprocess.run(["make", f"-j{min(os.cpu_count() or 1, 4)}", "libsqlite3.a", "sqlite3.h"], cwd=source, check=True)
        (PREFIX / "lib" / "pkgconfig").mkdir(parents=True, exist_ok=True)
        (PREFIX / "include").mkdir(parents=True, exist_ok=True)
        shutil.copy2(source / "libsqlite3.a", PREFIX / "lib" / "libsqlcipher.a")
        shutil.copy2(source / "sqlite3.h", PREFIX / "include" / "sqlite3.h")
        license_dir = PREFIX / "share" / "licenses" / "sqlcipher"
        license_dir.mkdir(parents=True, exist_ok=True)
        for name in ["LICENSE.md", "LICENSE.txt", "SQLITE_LICENSE.md"]:
            shutil.copy2(source / name, license_dir / name)
        sqlite_version = (source / "VERSION").read_text().strip()
        (PREFIX / "lib" / "pkgconfig" / "sqlcipher.pc").write_text(
            f"prefix={PREFIX}\nlibdir=${{prefix}}/lib\nincludedir=${{prefix}}/include\n"
            "Name: SQLCipher\nDescription: Trust pinned SQLCipher backend\n"
            f"Version: {sqlite_version}\nLibs: -L${{libdir}} -lsqlcipher -lcrypto -lm -lpthread -ldl\n"
            "Cflags: -I${includedir}\n"
        )
        (PREFIX / "source.sha256").write_text(f"{ARCHIVE_SHA256}  sqlcipher-{VERSION}.tar.gz\n")
    print(f"Built SQLCipher {VERSION} / SQLite {sqlite_version} at {PREFIX}")


if __name__ == "__main__":
    main()
