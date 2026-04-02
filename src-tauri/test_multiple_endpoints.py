#!/usr/bin/env python3
"""
测试多个可能的API端点
"""

import requests
import json

def test_endpoint(url, method="POST", headers=None, data=None):
    """测试单个端点"""
    try:
        if method == "GET":
            response = requests.get(url, headers=headers, timeout=10)
        else:
            response = requests.post(url, headers=headers, json=data, timeout=10)
        
        return {
            "url": url,
            "status": response.status_code,
            "headers": dict(response.headers),
            "body": response.text[:500] if response.text else ""
        }
    except Exception as e:
        return {
            "url": url,
            "error": str(e),
            "status": "connection_failed"
        }

def main():
    base_url = "https://chenyu.pro"
    api_key = "***REMOVED***06b6479e"
    
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json"
    }
    
    # 常见的LLM API端点
    endpoints = [
        "/api/v1/llm",
        "/api/chat/completions",  # OpenAI兼容格式
        "/api/generate",  # Ollama格式
        "/v1/chat/completions",
        "/chat/completions",
        "/api/v1/chat/completions",
        "/api/v1/completions",
        "/api/completions",
        "/api/models",  # 模型列表端点
        "/api/tags",  # Ollama tags端点
        "/api/version",  # 版本信息
        "/api/show",  # Ollama显示模型信息
        "/v1/models",  # OpenAI模型列表
        "/health",  # 健康检查
        "/"
    ]
    
    print(f"🔍 测试 {base_url} 上的API端点...")
    print(f"📁 API Key: {api_key[:12]}...{api_key[-4:]}")
    print("=" * 80)
    
    results = []
    
    # 首先测试GET请求的端点（不需要请求体）
    get_endpoints = ["/api/models", "/api/tags", "/api/version", "/api/show", "/v1/models", "/health", "/"]
    
    for endpoint in get_endpoints:
        url = base_url + endpoint
        print(f"📡 测试 GET {endpoint} ...")
        result = test_endpoint(url, method="GET", headers=headers)
        results.append(result)
        
        if result.get("status") == 200:
            print(f"   ✅ 成功: {result['status']}")
            if result.get("body"):
                print(f"   响应: {result['body'][:100]}...")
        elif "error" in result:
            print(f"   ❌ 错误: {result['error']}")
        else:
            print(f"   ⚠️  状态: {result.get('status', '未知')}")
    
    print("\n" + "=" * 80)
    print("📤 测试POST请求端点...")
    
    # 测试POST端点
    test_payload = {
        "model": "Jackrong/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-v2",
        "messages": [{"role": "user", "content": "Hello"}],
        "max_tokens": 10
    }
    
    for endpoint in [e for e in endpoints if e not in get_endpoints]:
        url = base_url + endpoint
        print(f"📡 测试 POST {endpoint} ...")
        result = test_endpoint(url, method="POST", headers=headers, data=test_payload)
        results.append(result)
        
        if result.get("status") == 200:
            print(f"   ✅ 成功: {result['status']}")
            if result.get("body"):
                print(f"   响应: {result['body'][:100]}...")
        elif "error" in result:
            print(f"   ❌ 错误: {result['error']}")
        else:
            print(f"   ⚠️  状态: {result.get('status', '未知')}")
            if result.get("body"):
                print(f"   响应: {result['body'][:100]}...")
    
    print("\n" + "=" * 80)
    print("📊 测试结果总结:")
    print("\n可用的端点:")
    
    available = []
    for result in results:
        if result.get("status") == 200:
            available.append(result["url"])
            print(f"  ✅ {result['url']}")
    
    if not available:
        print("  ❌ 没有找到可用的API端点")
        
        # 检查是否有授权问题
        auth_errors = []
        for result in results:
            if result.get("status") == 401:
                auth_errors.append(result["url"])
        
        if auth_errors:
            print(f"\n🔐 授权问题 (401) 出现在:")
            for url in auth_errors:
                print(f"  ⚠️  {url}")
            print("\n💡 API Key 可能是有效的，但端点路径不正确")
        else:
            print("\n💡 可能的原因:")
            print("  1. API服务已停止运行")
            print("  2. 域名或服务器配置已更改")
            print("  3. 需要特定的请求格式或参数")
    
    # 保存详细结果
    with open("endpoint_test_results.json", "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
    
    print(f"\n📁 详细结果已保存到: endpoint_test_results.json")

if __name__ == "__main__":
    main()