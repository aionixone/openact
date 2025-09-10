#!/usr/bin/env python3
"""
简单的回调服务器，用于捕获 GitHub OAuth2 授权码
"""

import http.server
import socketserver
import urllib.parse
import json
import sys
from urllib.parse import urlparse, parse_qs

class CallbackHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith('/oauth/callback'):
            # 解析查询参数
            parsed_url = urlparse(self.path)
            query_params = parse_qs(parsed_url.query)
            
            # 提取授权码
            code = query_params.get('code', [None])[0]
            state = query_params.get('state', [None])[0]
            error = query_params.get('error', [None])[0]
            
            if error:
                print(f"❌ OAuth 错误: {error}")
                self.send_response(400)
                self.send_header('Content-type', 'text/html')
                self.end_headers()
                self.wfile.write(f"<h1>OAuth 错误: {error}</h1>".encode())
                return
            
            if code:
                print(f"✅ 获取到授权码: {code}")
                print(f"📋 状态: {state}")
                
                # 保存授权码到文件
                with open('/tmp/github_auth_code.txt', 'w') as f:
                    f.write(code)
                
                self.send_response(200)
                self.send_header('Content-type', 'text/html')
                self.end_headers()
                self.wfile.write(b"""
                <html>
                <head><title>授权成功</title></head>
                <body>
                    <h1>✅ GitHub 授权成功！</h1>
                    <p>授权码已保存，可以继续 OAuth2 流程。</p>
                    <p>请返回终端查看结果。</p>
                </body>
                </html>
                """)
            else:
                print("❌ 未找到授权码")
                self.send_response(400)
                self.send_header('Content-type', 'text/html')
                self.end_headers()
                self.wfile.write(b"<h1>未找到授权码</h1>")
        else:
            self.send_response(404)
            self.end_headers()
    
    def log_message(self, format, *args):
        # 禁用默认日志
        pass

def main():
    PORT = 8080
    
    # 检查端口是否被占用
    try:
        with socketserver.TCPServer(("", PORT), CallbackHandler) as httpd:
            print(f"🔄 回调服务器启动在端口 {PORT}")
            print(f"📋 等待 GitHub 回调...")
            print(f"💡 请在浏览器中访问授权 URL")
            httpd.serve_forever()
    except OSError as e:
        if e.errno == 48:  # Address already in use
            print(f"❌ 端口 {PORT} 已被占用")
            print("💡 请确保 AuthFlow 服务器没有运行，或者使用不同的端口")
            sys.exit(1)
        else:
            raise

if __name__ == "__main__":
    main()
