#!/usr/bin/env python3
"""
测试不同的模型格式
"""

import requests
import json

url = "https://chenyu.pro/api/v1/llm"
api_key = "***REMOVED***06b6479e"

headers = {
    "Authorization": f"Bearer {api_key}",
    "Content-Type": "application/json"
}

# 可能的模型名称格式
model_formats = [
    # 原始格式
    "Jackrong/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-v2",
    
    # 可能需要的其他格式
    "qwen3.5-27b-claude-4.6-opus-reasoning-distilled-v2",
    "qwen-3.5-27b",
    "qwen3.5-27b",
    "qwen3.5",
    "Qwen3.5-27B",
    "jackrong-qwen3.5-27b",
    
    # Hugging Face样式
    "Jackrong/Qwen3.5-27B",
    "Jackrong/Qwen3.5",
    
    # 可能有版本号问题
    "Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled",
    "qwen3.5-27b-distilled",
    
    # 尝试通用模型名称
    "qwen",
    "chat-model",
    "default"
]

print("🔍 测试不同模型名称格式...")
print(f"📊 API端点: {url}")
print(f"🔑 API Key: {api_key[:12]}...{api_key[-4:]}")
print("="*80)

for model_name in model_formats:
    print(f"\n🧪 测试模型名称: {model_name}")
    
    data = {
        "model": model_name,
        "messages": [
            {"role": "user", "content": "Hello, test"}
        ],
        "max_tokens": 20
    }
    
    try:
        response = requests.post(url, headers=headers, json=data, timeout=10)
        print(f"   状态码: {response.status_code}")
        print(f"   响应类型: {response.headers.get('content-type', 'unknown')}")
        
        if response.status_code != 404:
            print(f"   ⚠️  非404响应! 响应: {response.text[:100]}")
            
            if response.status_code == 200:
                try:
                    json_data = response.json()
                    print(f"   ✅ JSON响应成功!")
                    print(f"       响应结构: {list(json_data.keys()) if isinstance(json_data, dict) else 'not dict'}")
                except:
                    print(f"   📄 响应内容: {response.text[:200]}")
                    
    except Exception as e:
        print(f"   ❌ 请求异常: {e}")

# 测试可能有不同的端点
print("\n" + "="*80)
print("🔍 尝试不同的API路径...")

# 也许问题在于端点路径
possible_paths = [
    "/api/v1/chat/completions",
    "/v1/chat/completions",
    "/api/chat/completions",
    "/chat/completions",
    "/api/completion",
    "/api/generate"
]

for path in possible_paths:
    test_url = f"https://chenyu.pro{path}"
    print(f"\n📡 测试: {test_url}")
    
    # 使用最可能正确的模型名称
    data = {
        "model": "qwen3.5-27b",
        "messages": [
            {"role": "user", "content": "Hello"}
        ],
        "max_tokens": 20
    }
    
    try:
        response = requests.post(test_url, headers=headers, json=data, timeout=10)
        print(f"   状态码: {response.status_code}")
        
        if response.status_code != 404 and response.status_code != 405:
            print(f"   ⚠️  非常规响应! 响应: {response.text[:100]}")
    except Exception as e:
        print(f"   ❌ 请求异常: {e}")