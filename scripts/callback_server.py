#!/usr/bin/env python3
"""
A simple callback server to capture GitHub OAuth2 authorization codes
"""

import http.server
import socketserver
import urllib.parse
import json
import sys
from urllib.parse import urlparse, parse_qs
import os

class CallbackHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith('/oauth/callback'):
            # Parse query parameters
            parsed_url = urlparse(self.path)
            query_params = parse_qs(parsed_url.query)
            
            # Extract authorization code
            code = query_params.get('code', [None])[0]
            state = query_params.get('state', [None])[0]
            error = query_params.get('error', [None])[0]
            
            if error:
                print(f"❌ OAuth error: {error}")
                self.send_response(400)
                self.send_header('Content-type', 'text/html; charset=utf-8')
                self.end_headers()
                html = f"<h1>OAuth error: {error}</h1>"
                self.wfile.write(html.encode('utf-8'))
                return
            
            if code:
                print(f"✅ Authorization code received: {code}")
                print(f"📋 State: {state}")
                
                # Save authorization code to file
                with open('/tmp/github_auth_code.txt', 'w') as f:
                    f.write(code)
                
                self.send_response(200)
                self.send_header('Content-type', 'text/html; charset=utf-8')
                self.end_headers()
                html = (
                    "<html>"
                    "<head><title>Authorization Successful</title></head>"
                    "<body>"
                    "<h1>✅ GitHub Authorization Successful!</h1>"
                    "<p>The authorization code has been saved. You can continue the OAuth2 process.</p>"
                    "<p>Please return to the terminal to see the results.</p>"
                    "</body>"
                    "</html>"
                )
                self.wfile.write(html.encode('utf-8'))
            else:
                print("❌ Authorization code not found")
                self.send_response(400)
                self.send_header('Content-type', 'text/html; charset=utf-8')
                self.end_headers()
                self.wfile.write("<h1>Authorization code not found</h1>".encode('utf-8'))
        else:
            self.send_response(404)
            self.end_headers()
    
    def log_message(self, format, *args):
        # Disable default logging
        pass

def main():
    PORT = int(os.environ.get("OPENACT_CALLBACK_PORT", "8080"))
    
    # Check if the port is already in use
    try:
        with socketserver.TCPServer(("", PORT), CallbackHandler) as httpd:
            print(f"🔄 Callback server started on port {PORT}")
            print(f"📋 Waiting for GitHub callback...")
            print(f"💡 Please visit the authorization URL in your browser")
            httpd.serve_forever()
    except OSError as e:
        if e.errno == 48:  # Address already in use
            print(f"❌ Port {PORT} is already in use")
            print("💡 Please ensure the openact server is not running, or use a different port")
            sys.exit(1)
        else:
            raise

if __name__ == "__main__":
    main()
