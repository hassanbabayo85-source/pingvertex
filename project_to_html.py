#!/usr/bin/env python3
"""
project_to_html.py

Gudanar da wannan a cikin babban fayil na project (uptime-monitor), ko ka ba shi
hanyar (path) zuwa project din a matsayin argument na farko.

Amfani:
    python3 project_to_html.py [PROJECT_ROOT] [OUTPUT.html]

Misali:
    python3 project_to_html.py . uptime-monitor-bundle.html
    python3 project_to_html.py /home/user/uptime-monitor bundle.html

Abin da yake yi:
  - Yana bincika fayilolin da ake bukata don GUDANAR da project (config, source
    code, migrations, Docker/Compose, deploy scripts, da sauransu).
  - Yana KEBE (ware) fayiloli/babban-fayiloli na banza: backups, .git, .bak
    files, cookies, logs, node_modules, target/, dist/ da sauransu.
  - Yana samar da HTML DAYA mai jerin fayiloli (sidebar) da ainihin abin da ke
    ciki (code) na kowanne, tare da escaping mai kyau.
"""

import sys
import os
import html
from pathlib import Path

# ---------------------------------------------------------------------------
# Saitunan da za ka iya gyarawa
# ---------------------------------------------------------------------------

# Kaddarorin fayiloli (extensions) da ake so a hada
INCLUDE_EXTS = {
    ".rs", ".toml", ".sql", ".yml", ".yaml", ".json", ".sh",
    ".css", ".html", ".js", ".md", ".env", ".cnf",
}

# Sunayen fayiloli na musamman da ake so ko da babu extension da aka lissafa
INCLUDE_EXACT_NAMES = {
    "Caddyfile", "Dockerfile", "Dockerfile.agent", "Dockerfile.api",
    "Dockerfile.frontend", "Dockerfile.migrate", "Dockerfile.worker",
    ".env.example", ".dockerignore", ".gitignore",
}

# Babban-fayiloli (directories) da za a KEBE gaba daya (sunan babban fayil, ba
# cikakken hanya ba - idan sunan ya bayyana a ko'ina cikin path, za a tsallake)
EXCLUDE_DIR_SUBSTRINGS = [
    ".git",
    "node_modules",
    "target",
    "dist",
    ".controlled-parent-migration",
    ".parent-dependency-backup",
    ".parent-dependency-security-backup",
    ".parent-dependency-work",
    ".worker6-backup",
    ".pingvertex-repair",
    "data/geoip",
]

# Fayiloli na musamman da za a KEBE (sunan fayil ko karshen suna)
EXCLUDE_FILE_PATTERNS = [
    ".bak", "cookies", "backend.log", "geoip.tar.gz", ".lock",
    "frontend.log", ".srl",
]

# Idan kana son a hada Cargo.lock ma, cire ".lock" daga jerin sama, amma yana
# da girma kuma ba dole ba ne don GUDANAR da project (cargo zai sake haifar
# dashi idan babu shi, ko a yi amfani da wanda ke wurin).


def is_excluded_dir(path_str: str) -> bool:
    for sub in EXCLUDE_DIR_SUBSTRINGS:
        if sub in path_str:
            return True
    return False


def is_excluded_file(name: str) -> bool:
    lower = name.lower()
    for pat in EXCLUDE_FILE_PATTERNS:
        if pat in lower:
            return True
    return False


def should_include(path: Path) -> bool:
    name = path.name
    if is_excluded_file(name):
        return False
    if name in INCLUDE_EXACT_NAMES:
        return True
    if path.suffix in INCLUDE_EXTS:
        return True
    return False


def collect_files(root: Path):
    collected = []
    for dirpath, dirnames, filenames in os.walk(root):
        rel_dir = os.path.relpath(dirpath, root)
        # Tsallake babban-fayiloli marasa muhimmanci gaba daya (don sauri)
        dirnames[:] = [
            d for d in dirnames
            if not is_excluded_dir(os.path.join(rel_dir, d))
        ]
        if is_excluded_dir(rel_dir):
            continue
        for fname in filenames:
            full = Path(dirpath) / fname
            rel = os.path.relpath(full, root)
            if is_excluded_dir(rel):
                continue
            if should_include(full):
                collected.append(full)
    collected.sort(key=lambda p: str(p).lower())
    return collected


def read_text_safe(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except Exception as e:
        return f"[Ba a iya karanta fayil din ba: {e}]"


def build_html(root: Path, files, out_path: Path):
    parts = []
    parts.append("<!DOCTYPE html>")
    parts.append("<html lang='ha'>")
    parts.append("<head>")
    parts.append("<meta charset='UTF-8'>")
    parts.append("<title>Uptime Monitor - Project Bundle</title>")
    parts.append("""
<style>
  :root { --bg:#0f1117; --panel:#161923; --text:#e6e6e6; --muted:#9aa0aa; --accent:#5cc8ff; --border:#2a2e3a; }
  * { box-sizing: border-box; }
  body { margin:0; font-family: -apple-system, Segoe UI, Roboto, Arial, sans-serif; background: var(--bg); color: var(--text); display:flex; height:100vh; }
  #sidebar { width: 340px; overflow-y:auto; background: var(--panel); border-right:1px solid var(--border); padding: 12px; }
  #sidebar h2 { font-size: 14px; color: var(--muted); text-transform:uppercase; letter-spacing:.05em; margin: 16px 4px 6px; }
  #sidebar input { width:100%; padding:8px; border-radius:6px; border:1px solid var(--border); background:#0c0e14; color:var(--text); margin-bottom:8px; }
  #sidebar a { display:block; padding:4px 6px; color: var(--text); text-decoration:none; font-size:13px; border-radius:4px; word-break: break-all; }
  #sidebar a:hover { background:#1f2430; }
  #sidebar a.active { background: #223047; color: var(--accent); }
  #content { flex:1; overflow-y:auto; padding: 20px 30px; }
  .file-block { margin-bottom: 40px; border:1px solid var(--border); border-radius:8px; overflow:hidden; }
  .file-header { background:#1b1f2a; padding:10px 14px; font-size:13px; color:var(--accent); border-bottom:1px solid var(--border); font-family: monospace; }
  pre { margin:0; padding:14px; overflow-x:auto; font-size:12.5px; line-height:1.5; background:#0c0e14; }
  code { font-family: 'Fira Code', Consolas, Menlo, monospace; white-space:pre; }
  ::selection { background:#3a5580; }
</style>
""")
    parts.append("</head>")
    parts.append("<body>")

    # Sidebar
    parts.append("<div id='sidebar'>")
    parts.append("<input id='searchBox' type='text' placeholder='Nemo fayil...' oninput='filterList()'>")
    parts.append("<div id='linkList'>")
    last_dir = None
    for i, f in enumerate(files):
        rel = os.path.relpath(f, root)
        top_dir = rel.split(os.sep)[0] if os.sep in rel else "(root)"
        if top_dir != last_dir:
            parts.append(f"<h2>{html.escape(top_dir)}</h2>")
            last_dir = top_dir
        anchor = f"file-{i}"
        parts.append(f"<a href='#{anchor}' data-name='{html.escape(rel.lower())}'>{html.escape(rel)}</a>")
    parts.append("</div>")
    parts.append("</div>")

    # Content
    parts.append("<div id='content'>")
    parts.append(f"<h1>Uptime Monitor &mdash; Project Bundle</h1>")
    parts.append(f"<p style='color:var(--muted)'>Jimillar fayiloli: {len(files)}</p>")
    for i, f in enumerate(files):
        rel = os.path.relpath(f, root)
        anchor = f"file-{i}"
        content = read_text_safe(f)
        parts.append(f"<div class='file-block' id='{anchor}'>")
        parts.append(f"<div class='file-header'>{html.escape(rel)}</div>")
        parts.append(f"<pre><code>{html.escape(content)}</code></pre>")
        parts.append("</div>")
    parts.append("</div>")

    parts.append("""
<script>
function filterList() {
  const q = document.getElementById('searchBox').value.toLowerCase();
  document.querySelectorAll('#linkList a').forEach(a => {
    a.style.display = a.dataset.name.includes(q) ? 'block' : 'none';
  });
}
</script>
""")
    parts.append("</body></html>")

    out_path.write_text("\n".join(parts), encoding="utf-8")


def main():
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(".").resolve()
    out = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else root / "project-bundle.html"

    if not root.exists():
        print(f"Babu wannan hanyar: {root}")
        sys.exit(1)

    print(f"Ana bincika fayiloli a: {root}")
    files = collect_files(root)
    print(f"An samu fayiloli {len(files)} da suka dace.")
    build_html(root, files, out)
    print(f"An kammala! Fayil din HTML: {out}")


if __name__ == "__main__":
    main()
