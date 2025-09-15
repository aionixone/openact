#!/usr/bin/env python3
"""
简单的STDIO-RPC客户端测试脚本
测试OpenAct STDIO-RPC接口的基本功能
"""

import json
import subprocess
import sys
import time

def send_rpc_request(process, method, params=None, request_id=1):
    """发送JSON-RPC请求并获取响应"""
    request = {
        "jsonrpc": "2.0",
        "method": method,
        "id": request_id
    }
    if params:
        request["params"] = params
    
    request_json = json.dumps(request) + "\n"
    print(f"📤 Sending: {request_json.strip()}")
    
    process.stdin.write(request_json.encode())
    process.stdin.flush()
    
    # 读取响应
    response_line = process.stdout.readline().decode().strip()
    print(f"📥 Received: {response_line}")
    
    try:
        response = json.loads(response_line)
        return response
    except json.JSONDecodeError as e:
        print(f"❌ Failed to parse response: {e}")
        return None

def test_stdio_rpc():
    """测试STDIO-RPC接口的基本功能"""
    print("🚀 Starting OpenAct STDIO-RPC tests...")
    
    # 设置环境变量
    env = {
        "OPENACT_DATABASE_URL": "sqlite:./data/openact.db",
        "OPENACT_MASTER_KEY": "your-32-byte-key-here-for-testing",
        "RUST_LOG": "info"
    }
    
    try:
        # 启动STDIO-RPC进程
        process = subprocess.Popen(
            ["cargo", "run", "-q", "-p", "openact-stdio"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            text=False  # 使用二进制模式以避免编码问题
        )
        
        # 等待进程启动
        time.sleep(2)
        
        print("✅ STDIO-RPC process started")
        
        # 测试1: 健康检查
        print("\n🔍 Test 1: Health check")
        response = send_rpc_request(process, "health")
        if response and "result" in response:
            print("✅ Health check passed")
        else:
            print("❌ Health check failed")
        
        # 测试2: 系统状态
        print("\n🔍 Test 2: System status")
        response = send_rpc_request(process, "status")
        if response and "result" in response:
            print("✅ Status check passed")
        else:
            print("❌ Status check failed")
        
        # 测试3: 系统诊断
        print("\n🔍 Test 3: System doctor")
        response = send_rpc_request(process, "doctor")
        if response and "result" in response:
            print("✅ Doctor check passed")
        else:
            print("❌ Doctor check failed")
        
        # 测试4: 无效方法
        print("\n🔍 Test 4: Invalid method")
        response = send_rpc_request(process, "invalid_method")
        if response and "error" in response:
            print("✅ Invalid method error handling passed")
        else:
            print("❌ Invalid method error handling failed")
            
        # 测试5: Action注册 (需要有效的配置文件)
        print("\n🔍 Test 5: Action registration (placeholder)")
        response = send_rpc_request(process, "action.register", {
            "config_path": "./providers/github/actions/get-user.yaml",
            "tenant": "test",
            "provider": "github", 
            "name": "get-user"
        })
        if response:
            if "result" in response:
                print("✅ Action registration passed")
            elif "error" in response:
                print(f"⚠️ Action registration failed (expected): {response['error']['message']}")
        
        print("\n🎉 All tests completed!")
        
    except Exception as e:
        print(f"❌ Test failed: {e}")
    finally:
        if 'process' in locals():
            process.terminate()
            process.wait()
            print("🛑 STDIO-RPC process terminated")

if __name__ == "__main__":
    test_stdio_rpc()
