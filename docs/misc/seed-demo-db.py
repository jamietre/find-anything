#!/usr/bin/env python3
"""
Seed find-anything source databases with synthetic file records for demo screenshots.

Inserts fake files with realistic sizes, kinds, and extract times, and builds a
scan_history covering the past year so the "Files over time" chart looks good.

Usage:
    python3 docs/misc/seed-demo-db.py [--data-dir DIR] [--source NAME] [--count N] [--clear]

Defaults:
    --data-dir  ~/.local/share/find-anything
    --source    projects
    --count     500
"""

import argparse
import json
import math
import os
import random
import sqlite3
import time
from pathlib import Path

# ── Realistic file distributions ──────────────────────────────────────────────

KIND_WEIGHTS = {
    "text":       35,
    "image":      25,
    "video":      12,
    "audio":       8,
    "pdf":         6,
    "archive":     4,
    "document":    4,
    "binary":      4,
    "executable":  1,
    "unknown":     1,
}

# (min_bytes, max_bytes) per kind
KIND_SIZE_RANGE = {
    "text":       (512,        500_000),
    "image":      (80_000,    25_000_000),
    "video":      (5_000_000, 4_000_000_000),
    "audio":      (2_000_000,   80_000_000),
    "pdf":        (50_000,    30_000_000),
    "archive":    (10_000,   500_000_000),
    "document":   (20_000,     5_000_000),
    "binary":     (4_000,    200_000_000),
    "executable": (50_000,   250_000_000),
    "unknown":    (100,        1_000_000),
}

# (min_ms, max_ms) extraction time; None = not extracted
KIND_EXTRACT_MS = {
    "text":       (1,    120),
    "image":      (5,    300),
    "video":      (None, None),
    "audio":      (5,     80),
    "pdf":        (80,  8_000),
    "archive":    (20,  1_500),
    "document":   (30,  3_000),
    "binary":     (None, None),
    "executable": (None, None),
    "unknown":    (None, None),
}

KIND_EXTENSIONS = {
    "text":       ["rs", "py", "ts", "js", "go", "java", "md", "txt", "yaml",
                   "toml", "json", "sh", "sql", "html", "css", "rb", "cpp", "c"],
    "image":      ["jpg", "jpeg", "png", "heic", "tiff", "raw", "cr2", "webp"],
    "video":      ["mp4", "mkv", "mov", "avi", "m4v", "webm"],
    "audio":      ["mp3", "flac", "m4a", "ogg", "wav", "opus"],
    "pdf":        ["pdf"],
    "archive":    ["zip", "tar.gz", "7z", "tgz", "tar.bz2"],
    "document":   ["docx", "xlsx", "pptx", "epub"],
    "binary":     ["bin", "db", "sqlite", "wasm", "so", "dylib", "ttf"],
    "executable": ["exe", "dll", "deb", "rpm"],
    "unknown":    ["dat", "bak", "tmp", "cache"],
}

# Plausible directory structures per kind
KIND_DIRS = {
    "text": [
        "src", "src/api", "src/auth", "src/db", "src/models", "src/utils",
        "tests", "docs", "scripts", "config", "lib", "internal",
    ],
    "image": [
        "assets/images", "assets/icons", "public/img", "photos", "screenshots",
        "media", "resources",
    ],
    "video": [
        "recordings", "media/video", "assets/video", "tutorials",
    ],
    "audio": [
        "media/audio", "music", "podcasts", "recordings", "assets/sounds",
    ],
    "pdf": [
        "docs", "reports", "contracts", "invoices", "manuals", "research",
    ],
    "archive": [
        "releases", "backups", "dist", "artifacts", "vendor",
    ],
    "document": [
        "docs", "reports", "presentations", "specs", "proposals",
    ],
    "binary": [
        "lib", "vendor", "bin", "data", "cache", "build",
    ],
    "executable": [
        "bin", "dist", "build/release",
    ],
    "unknown": [
        "data", "tmp", "cache", "misc",
    ],
}

# Plausible file name stems per kind
KIND_STEMS = {
    "text": [
        "main", "lib", "index", "utils", "config", "auth", "api", "db", "server",
        "client", "routes", "handler", "middleware", "schema", "migration",
        "model", "service", "controller", "test", "spec", "README", "CHANGELOG",
        "ARCHITECTURE", "Makefile", "Dockerfile", "deploy", "setup", "init",
        "parser", "lexer", "encoder", "decoder", "cache", "queue", "worker",
        "scheduler", "notifier", "webhook", "session", "token", "crypto",
    ],
    "image": [
        "screenshot", "banner", "logo", "icon", "thumbnail", "avatar",
        "background", "hero", "diagram", "chart", "photo", "cover",
        "preview", "mockup", "wireframe",
        "IMG_0042", "IMG_1337", "DSC_0012", "DSC_2048", "DCIM_0099",
    ],
    "video": [
        "demo", "tutorial", "recording", "walkthrough", "presentation",
        "screen-capture", "review", "overview", "intro", "teaser",
    ],
    "audio": [
        "podcast-ep01", "podcast-ep02", "meeting-recording", "voiceover",
        "soundtrack", "ambient", "notification", "alert",
    ],
    "pdf": [
        "report", "invoice", "contract", "spec", "manual", "proposal",
        "architecture", "requirements", "runbook", "sla", "terms",
        "Q1-report", "Q2-report", "Q3-report", "Q4-report",
    ],
    "archive": [
        "backup", "release-v1.0", "release-v1.1", "dist", "vendor",
        "assets", "exports", "archive-2023", "archive-2024",
    ],
    "document": [
        "spec", "proposal", "roadmap", "presentation", "report",
        "onboarding", "handbook", "playbook", "meeting-notes",
    ],
    "binary": [
        "libc", "libssl", "libcrypto", "database", "cache", "data",
        "model", "weights", "index",
    ],
    "executable": [
        "server", "worker", "migrate", "setup", "installer",
    ],
    "unknown": [
        "data", "output", "dump", "export", "import", "cache",
    ],
}


def pick_kind() -> str:
    kinds = list(KIND_WEIGHTS.keys())
    weights = [KIND_WEIGHTS[k] for k in kinds]
    return random.choices(kinds, weights=weights, k=1)[0]


def pick_size(kind: str) -> int:
    lo, hi = KIND_SIZE_RANGE[kind]
    # Log-uniform so we get a realistic spread rather than everything near the mean
    return int(math.exp(random.uniform(math.log(lo), math.log(hi))))


def pick_extract_ms(kind: str) -> int | None:
    lo, hi = KIND_EXTRACT_MS[kind]
    if lo is None:
        return None
    return int(math.exp(random.uniform(math.log(max(lo, 1)), math.log(max(hi, 1)))))


def pick_path(kind: str, used: set) -> str:
    for _ in range(100):
        directory = random.choice(KIND_DIRS[kind])
        stem = random.choice(KIND_STEMS[kind])
        ext_raw = random.choice(KIND_EXTENSIONS[kind])
        # Some stems get a number suffix to avoid collisions
        if random.random() < 0.4:
            stem = f"{stem}-{random.randint(1, 99):02d}"
        # Extensions like "tar.gz" need special handling
        ext = ext_raw if "." not in ext_raw else ext_raw
        path = f"{directory}/{stem}.{ext}"
        if path not in used:
            used.add(path)
            return path
    # Fallback with uuid-ish suffix
    path = f"{random.choice(KIND_DIRS[kind])}/{stem}-{random.randint(1000,9999)}.{ext}"
    used.add(path)
    return path


# ── scan_history helpers ───────────────────────────────────────────────────────

def build_by_kind_json(files: list[dict]) -> str:
    """Aggregate files into the by_kind JSON format expected by the server."""
    agg: dict[str, dict] = {}
    for f in files:
        k = f["kind"]
        if k not in agg:
            agg[k] = {"count": 0, "size": 0, "avg_extract_ms": None, "_ms_sum": 0, "_ms_count": 0}
        agg[k]["count"] += 1
        agg[k]["size"] += f["size"]
        if f["extract_ms"] is not None:
            agg[k]["_ms_sum"] += f["extract_ms"]
            agg[k]["_ms_count"] += 1

    result = {}
    for k, v in agg.items():
        avg = v["_ms_sum"] / v["_ms_count"] if v["_ms_count"] > 0 else None
        result[k] = {"count": v["count"], "size": v["size"], "avg_extract_ms": avg}
    return json.dumps(result)


def make_scan_history(all_files: list[dict], n_scans: int = 24) -> list[dict]:
    """
    Build n_scans history points spread over the past year.
    File counts grow organically — each scan sees the files that were
    indexed_at <= that scan's timestamp.
    """
    now = int(time.time())
    one_year_ago = now - 365 * 86400

    # Evenly-spaced scan timestamps over the past year
    scan_times = [
        int(one_year_ago + i * (now - one_year_ago) / (n_scans - 1))
        for i in range(n_scans)
    ]

    points = []
    for ts in scan_times:
        visible = [f for f in all_files if f["indexed_at"] <= ts]
        if not visible:
            continue
        total_files = len(visible)
        total_size = sum(f["size"] for f in visible)
        by_kind = build_by_kind_json(visible)
        points.append({
            "scanned_at": ts,
            "total_files": total_files,
            "total_size": total_size,
            "by_kind": by_kind,
        })
    return points


# ── Database operations ────────────────────────────────────────────────────────

def ensure_schema(conn: sqlite3.Connection) -> None:
    """Make sure the required columns exist (in case DB was created by an older version)."""
    cur = conn.execute("PRAGMA table_info(files)")
    cols = {row[1] for row in cur.fetchall()}
    if "indexed_at" not in cols:
        conn.execute("ALTER TABLE files ADD COLUMN indexed_at INTEGER")
    if "extract_ms" not in cols:
        conn.execute("ALTER TABLE files ADD COLUMN extract_ms INTEGER")
    if "content_hash" not in cols:
        conn.execute("ALTER TABLE files ADD COLUMN content_hash TEXT")
    if "canonical_file_id" not in cols:
        conn.execute("ALTER TABLE files ADD COLUMN canonical_file_id INTEGER")
    conn.commit()


def seed(db_path: str, count: int, clear: bool) -> None:
    print(f"\nSeeding {db_path} with {count} files…")
    conn = sqlite3.connect(db_path)
    conn.execute("PRAGMA journal_mode=WAL")
    ensure_schema(conn)

    if clear:
        conn.execute("DELETE FROM scan_history")
        conn.execute("DELETE FROM files")
        conn.commit()
        print("  cleared existing data")

    # Find paths already in the DB so we don't collide
    existing_paths = {row[0] for row in conn.execute("SELECT path FROM files")}

    now = int(time.time())
    one_year_ago = now - 365 * 86400

    fake_files = []
    used_paths = set(existing_paths)

    for _ in range(count):
        kind = pick_kind()
        path = pick_path(kind, used_paths)
        size = pick_size(kind)
        extract_ms = pick_extract_ms(kind)
        # indexed_at: log-biased toward more recent (more files added recently)
        age_fraction = random.betavariate(1.5, 1.0)  # skewed toward recent
        indexed_at = int(one_year_ago + age_fraction * (now - one_year_ago))
        mtime = indexed_at - random.randint(0, 30 * 86400)  # mtime slightly before index time

        fake_files.append({
            "path":       path,
            "mtime":      mtime,
            "size":       size,
            "kind":       kind,
            "indexed_at": indexed_at,
            "extract_ms": extract_ms,
        })

    # Insert files
    before = conn.execute("SELECT COUNT(*) FROM files").fetchone()[0]
    conn.executemany(
        "INSERT OR IGNORE INTO files (path, mtime, size, kind, indexed_at, extract_ms) "
        "VALUES (:path, :mtime, :size, :kind, :indexed_at, :extract_ms)",
        fake_files,
    )
    after = conn.execute("SELECT COUNT(*) FROM files").fetchone()[0]
    print(f"  inserted {after - before} file records ({after} total)")

    # Rebuild scan history from all files (existing + new)
    all_db_files = [
        {"kind": r[0], "size": r[1], "extract_ms": r[2], "indexed_at": r[3] or now}
        for r in conn.execute("SELECT kind, size, extract_ms, indexed_at FROM files")
    ]

    conn.execute("DELETE FROM scan_history")
    history = make_scan_history(all_db_files, n_scans=24)
    conn.executemany(
        "INSERT INTO scan_history (scanned_at, total_files, total_size, by_kind) "
        "VALUES (:scanned_at, :total_files, :total_size, :by_kind)",
        history,
    )
    print(f"  wrote {len(history)} scan history points (past year)")

    conn.commit()
    conn.close()

    # Summary
    by_kind: dict[str, int] = {}
    for f in fake_files:
        by_kind[f["kind"]] = by_kind.get(f["kind"], 0) + 1
    print("  kinds: " + "  ".join(f"{k}={v}" for k, v in sorted(by_kind.items())))


# ── Entry point ────────────────────────────────────────────────────────────────

def main() -> None:
    default_data_dir = os.path.expanduser("~/.local/share/find-anything")

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", default=default_data_dir)
    parser.add_argument("--source", default=None,
                        help="Source name (default: seed all sources found in data-dir)")
    parser.add_argument("--count", type=int, default=500,
                        help="Number of fake file records to insert per source")
    parser.add_argument("--clear", action="store_true",
                        help="Delete existing file records before seeding")
    args = parser.parse_args()

    sources_dir = Path(args.data_dir) / "sources"
    if not sources_dir.exists():
        print(f"No sources directory found at {sources_dir}")
        print("Run find-server at least once to initialise it.")
        return

    if args.source:
        db_files = [sources_dir / f"{args.source}.db"]
    else:
        db_files = sorted(sources_dir.glob("*.db"))

    if not db_files:
        print(f"No .db files found in {sources_dir}")
        return

    for db_path in db_files:
        if db_path.exists():
            seed(str(db_path), args.count, args.clear)
        else:
            print(f"  skipping {db_path} (not found)")

    print("\nDone. Reload the stats page to see the updated data.")


if __name__ == "__main__":
    random.seed(42)
    main()
