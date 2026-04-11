#!/usr/bin/env python3
"""
本地更新测试服务器

模拟 Tauri updater endpoint，用于开发调试自动更新功能。
同时支持安装版（NSIS）和便携版（portable）的更新检查。

用法:
    python scripts/update_server.py                    # 默认模拟版本 99.0.0
    python scripts/update_server.py --version 2.0.0    # 指定模拟版本
    python scripts/update_server.py --no-update        # 模拟"无更新"（始终 204）

端点:
    GET /update/{target}/{arch}/{current_version}
        - 返回 JSON 更新信息（如果有更新）
        - 返回 204（如果已是最新）
        - 响应同时包含 url (NSIS 安装包) 和 portable_url (便携版 exe)
"""

import json
import argparse
from http.server import HTTPServer, BaseHTTPRequestHandler
from datetime import datetime, timezone


def parse_args():
    parser = argparse.ArgumentParser(description="本地更新测试服务器")
    parser.add_argument("--port", type=int, default=8787, help="监听端口 (默认 8787)")
    parser.add_argument(
        "--version", default="99.0.0", help="模拟的最新版本号 (默认 99.0.0)"
    )
    parser.add_argument("--no-update", action="store_true", help="始终返回无更新 (204)")
    parser.add_argument("--notes", default=None, help="更新日志内容")
    parser.add_argument(
        "--url", default=None, help="安装包下载 URL（可选，默认生成占位 URL）"
    )
    parser.add_argument(
        "--portable-url", default=None, help="便携版下载 URL（可选，默认生成占位 URL）"
    )
    parser.add_argument(
        "--signature", default="", help="安装包签名（可选，测试时可留空）"
    )
    return parser.parse_args()


class UpdateHandler(BaseHTTPRequestHandler):
    """处理 Tauri updater 请求"""

    def do_GET(self):
        # 解析路径: /update/{target}/{arch}/{current_version}
        parts = self.path.strip("/").split("/")

        if len(parts) == 4 and parts[0] == "update":
            target, arch, current_version = parts[1], parts[2], parts[3]
            self.handle_update_check(target, arch, current_version)
        else:
            self.send_response(404)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"Not Found")

    def handle_update_check(self, target, arch, current_version):
        args = self.server.args

        print(
            f"[检查更新] target={target} arch={arch} current={current_version} latest={args.version}"
        )

        # 模拟无更新
        if args.no_update or current_version == args.version:
            print(f"  -> 204 无更新")
            self.send_response(204)
            self.end_headers()
            return

        # 简单版本比较
        try:
            current_parts = tuple(int(x) for x in current_version.split("."))
            latest_parts = tuple(int(x) for x in args.version.split("."))
            if current_parts >= latest_parts:
                print(f"  -> 204 当前版本已是最新")
                self.send_response(204)
                self.end_headers()
                return
        except ValueError:
            pass

        # 构造更新响应
        notes = (
            args.notes
            or f"### v{args.version} 更新内容\n\n- 这是测试更新服务器生成的模拟更新\n- 新增自动更新功能\n- 修复若干已知问题"
        )

        # NSIS 安装包 URL
        nsis_url = (
            args.url
            or f"http://localhost:{args.port}/releases/danmuji-next_{args.version}_x64-setup.nsis.zip"
        )

        # 便携版 exe URL
        portable_url = (
            args.portable_url
            or f"http://localhost:{args.port}/releases/danmuji-next_v{args.version}_windows_x64_portable.exe"
        )

        response = {
            "version": args.version,
            "notes": notes,
            "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "url": nsis_url,
            "signature": args.signature,
            "portable_url": portable_url,
        }

        body = json.dumps(response, ensure_ascii=False, indent=2).encode("utf-8")

        print(f"  -> 200 发现更新 v{args.version}")
        print(f"     url:          {nsis_url}")
        print(f"     portable_url: {portable_url}")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        # 静默默认日志，用自定义 print 代替
        pass


def main():
    args = parse_args()

    server = HTTPServer(("0.0.0.0", args.port), UpdateHandler)
    server.args = args

    mode = "始终无更新 (204)" if args.no_update else f"模拟最新版本 v{args.version}"
    print(f"=" * 50)
    print(f"  弹幕姬 本地更新测试服务器")
    print(f"=" * 50)
    print(f"  地址:     http://localhost:{args.port}")
    print(f"  端点:     /update/{{target}}/{{arch}}/{{current_version}}")
    print(f"  模式:     {mode}")
    print(f"  安装版:   返回 url (NSIS)")
    print(f"  便携版:   返回 portable_url (exe)")
    print(f"=" * 50)
    print(f"  等待请求中...\n")

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n服务器已停止")
        server.server_close()


if __name__ == "__main__":
    main()
