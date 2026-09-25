"""下载参考站公开的 HTML / JS / CSS，保留路径供离线阅读。
用法：python scripts/download_timer_reference.py
只抓取静态代码，不连接直播接口或下载直播间数据。
"""
import argparse
import json
import re
from collections import deque
from pathlib import Path
from urllib.parse import unquote, urljoin, urlsplit
from urllib.request import Request, urlopen


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="https://xiaofaduo.com/client/timerRoom/bl/FPTI74K71ZB41")
    parser.add_argument("--output", default=".reference/timer")
    parser.add_argument("--max-files", type=int, default=500)
    args = parser.parse_args()
    root = Path(args.output).resolve()
    root.mkdir(parents=True, exist_ok=True)
    origin = urlsplit(args.url).netloc
    queue, seen, manifest = deque([args.url]), set(), []
    pattern = re.compile(r"[\"']([^\"'\s<>]+\.(?:js|css)(?:\?[^\"'\s<>]*)?)[\"']")
    while queue and len(seen) < args.max_files:
        url = queue.popleft()
        if url in seen:
            continue
        seen.add(url)
        try:
            with urlopen(Request(url, headers={"User-Agent": "danmuji-reference-downloader/1.0"}), timeout=30) as response:
                content = response.read(12 * 1024 * 1024 + 1)
                if len(content) > 12 * 1024 * 1024:
                    raise ValueError("单个资源超过 12 MiB")
            relative = "index.html" if url == args.url else unquote(urlsplit(url).path).lstrip("/")
            target = (root / relative).resolve()
            if not target.is_relative_to(root):
                raise ValueError("非法资源路径")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
            manifest.append({"url": url, "file": relative, "bytes": len(content)})
            for reference in pattern.findall(content.decode("utf-8", errors="replace")):
                base = f"https://{origin}/" if reference.startswith("assets/") else url
                dependency = urljoin(base, reference)
                if urlsplit(dependency).netloc == origin and dependency not in seen:
                    queue.append(dependency)
            print(f"已下载 {relative} ({len(content)} 字节)")
        except Exception as error:
            manifest.append({"url": url, "error": str(error)})
            print(f"下载失败 {url}: {error}")
    (root / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"完成：{len(manifest)} 个记录；剩余队列 {len(queue)}；目录 {root}")


if __name__ == "__main__":
    main()
